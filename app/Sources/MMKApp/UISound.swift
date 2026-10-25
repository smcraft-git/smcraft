import AVFoundation
import Foundation

/// Plays the UI sounds the front end asks for (`mmk_front_sounds`: a WAV of the SMM2 or the SMM1
/// import's `ui/sound/`, a volume, a delay in frames). Which sound a button or animation plays is
/// decided by `mmk-front` (core/crates/front/src/sound.rs, smm1_sound.rs) from the import's
/// `sounds.json`. The menu music (`MmkFrontSound.kind` 1–3) plays on its own player node: once,
/// or from its start and then round its WAV's `smpl` loop; a track switch keeps the position.
@MainActor
final class UISound {
    static let shared = UISound()

    private let dir: URL?
    private let smm1Dir: URL?
    private var players: [AVAudioPlayer] = []
    private var ends: [ObjectIdentifier: TimeInterval] = [:]
    private var cache: [String: Data] = [:]

    /// Debug: `MMK_DEBUG_SOUND=1` logs every UI sound to stderr.
    private static let log = ProcessInfo.processInfo.environment["MMK_DEBUG_SOUND"] == "1"
    private static let muted = ProcessInfo.processInfo.environment["MMK_MUTE"] == "1"

    private init() {
        dir = AppModel.smm2UIDir?.appendingPathComponent("ui/sound", isDirectory: true)
        smm1Dir = AppPaths.findSMM1Cache()?.appendingPathComponent("ui/sound", isDirectory: true)
    }

    // MARK: menu music

    private var engine: AVAudioEngine?
    private var node: AVAudioPlayerNode?
    private var musicBuffer: AVAudioPCMBuffer?
    /// The loop (first frame, frame after the last) of the music playing.
    private var musicLoop: (Int, Int)?
    /// The frame the node's playback started from.
    private var musicFrom = 0
    private var fadeTimer: Timer?

    /// The first `smpl` loop of a WAV: (start, end + 1).
    private static func smplLoop(_ d: Data) -> (Int, Int)? {
        var pos = 12
        let b = [UInt8](d)
        func u32(_ o: Int) -> Int { Int(b[o]) | Int(b[o + 1]) << 8 | Int(b[o + 2]) << 16 | Int(b[o + 3]) << 24 }
        while pos + 8 <= b.count {
            let n = u32(pos + 4)
            if b[pos..<pos + 4].elementsEqual("smpl".utf8), n >= 60, u32(pos + 8 + 28) > 0 {
                return (u32(pos + 8 + 44), u32(pos + 8 + 48) + 1)
            }
            pos += 8 + n + (n & 1)
        }
        return nil
    }

    /// The music's position (frames into its WAV), counting its loop.
    private func musicPosition() -> Int {
        guard let node, let t = node.lastRenderTime, let p = node.playerTime(forNodeTime: t) else { return 0 }
        var at = musicFrom + Int(p.sampleTime)
        if let (a, e) = musicLoop, at >= e { at = a + (at - a) % max(1, e - a) }
        return at
    }

    /// Start `sound` as the menu music (`samePosition`: a track switch, at the position reached).
    func music(_ sound: String, volume: Float, smm1: Bool, samePosition: Bool) {
        guard let dir = smm1 ? smm1Dir : dir, !Self.muted else { return }
        let url = dir.appendingPathComponent("\(sound).wav")
        guard let data = try? Data(contentsOf: url), let file = try? AVAudioFile(forReading: url),
              let buf = AVAudioPCMBuffer(pcmFormat: file.processingFormat,
                                         frameCapacity: AVAudioFrameCount(file.length)),
              (try? file.read(into: buf)) != nil else { return }
        let from = samePosition && fadeTimer == nil ? musicPosition() : 0
        stopMusicNow()
        let engine = self.engine ?? AVAudioEngine()
        self.engine = engine
        let node = AVAudioPlayerNode()
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: buf.format)
        if !engine.isRunning { try? engine.start() }
        let ud = UserDefaults.standard
        func vol(_ k: String) -> Float { ud.object(forKey: k) == nil ? 1 : Float(ud.double(forKey: k)) }
        node.volume = volume * vol(SettingsKey.volumeMaster) * vol(SettingsKey.volumeMusic)
        let loop = Self.smplLoop(data).flatMap { $0.0 < $0.1 && $0.1 <= Int(buf.frameLength) ? $0 : nil }
        func part(_ a: Int, _ e: Int) -> AVAudioPCMBuffer? {
            guard e > a, let p = AVAudioPCMBuffer(pcmFormat: buf.format, frameCapacity: AVAudioFrameCount(e - a)),
                  let src = buf.floatChannelData, let dst = p.floatChannelData else { return nil }
            for c in 0..<Int(buf.format.channelCount) { dst[c].update(from: src[c] + a, count: e - a) }
            p.frameLength = AVAudioFrameCount(e - a)
            return p
        }
        let end = loop?.1 ?? Int(buf.frameLength)
        let start = min(from, max(0, end - 1))
        if let head = part(start, end) { node.scheduleBuffer(head) }
        if let (a, e) = loop, let body = part(a, e) { node.scheduleBuffer(body, at: nil, options: .loops) }
        node.play()
        self.node = node
        musicBuffer = buf
        musicLoop = loop
        musicFrom = start
    }

    private func stopMusicNow() {
        fadeTimer?.invalidate()
        fadeTimer = nil
        if let node {
            node.stop()
            engine?.detach(node)
        }
        node = nil
        musicBuffer = nil
    }

    /// Fade the menu music out over `fadeFrames` (at once with 0).
    func stopMusic(fadeFrames: Double) {
        guard let node, fadeTimer == nil else { return }
        if fadeFrames <= 0 { stopMusicNow(); return }
        let start = node.volume
        var left = fadeFrames
        fadeTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                left -= 1
                if left <= 0 { self.stopMusicNow(); return }
                self.node?.volume = start * Float(left / fadeFrames)
            }
        }
    }

    func play(_ sound: String, volume: Float, delayFrames: Double, smm1: Bool = false) {
        guard sound != "@Blank", let dir = smm1 ? smm1Dir : dir, !Self.muted else { return }
        if Self.log {
            FileHandle.standardError.write("UISound \(smm1 ? "smm1/" : "")\(sound) volume \(volume) delay \(delayFrames)\n".data(using: .utf8)!)
        }
        let key = smm1 ? "smm1/\(sound)" : sound
        let data: Data
        if let c = cache[key] {
            data = c
        } else {
            guard let d = try? Data(contentsOf: dir.appendingPathComponent("\(sound).wav")) else { return }
            cache[key] = d
            data = d
        }
        guard let p = try? AVAudioPlayer(data: data) else { return }
        let ud = UserDefaults.standard
        func vol(_ k: String) -> Float { ud.object(forKey: k) == nil ? 1 : Float(ud.double(forKey: k)) }
        p.volume = volume * vol(SettingsKey.volumeMaster) * vol(SettingsKey.volumeSfx)
        p.prepareToPlay()
        // keep each player until its sound has played out
        let now = p.deviceCurrentTime
        players.removeAll { $0.deviceCurrentTime > ends[ObjectIdentifier($0)] ?? 0 }
        ends = ends.filter { id, _ in players.contains { ObjectIdentifier($0) == id } }
        players.append(p)
        ends[ObjectIdentifier(p)] = now + delayFrames / 60 + p.duration + 0.5
        if delayFrames > 0 {
            p.play(atTime: now + delayFrames / 60)
        } else {
            p.play()
        }
    }
}

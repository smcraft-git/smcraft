//! The front end's UI sounds (`mmk_front::SoundCue`: WAVs of the SMM2 UI import's
//! `ui/sound/`, or the SMM1 import's for the SMM1 screens), mixed into the same output stream as
//! the game's sound: a small voice list of 16-bit PCM clips with a gain and a start delay,
//! resampled linearly to the device rate; and one menu music voice (`CueKind::Music`) that loops
//! between its WAV's `smpl` loop points, can switch tracks in place and fades out.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mmk_front::UiKind;

/// A decoded clip: interleaved stereo at `rate` Hz, with its loop (`smpl`: first frame, frame
/// after the last) if the WAV has one.
#[derive(Clone, Debug)]
pub struct Clip {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
    pub looping: Option<(usize, usize)>,
}

/// Parse a PCM WAV (8/16-bit, mono or stereo).
pub fn parse_wav(b: &[u8]) -> Option<Clip> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let (mut channels, mut rate, mut bits) = (0u16, 0u32, 0u16);
    let mut looping = None;
    let mut frames = None;
    let mut pos = 12;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let n = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = b.get(pos + 8..(pos + 8 + n).min(b.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"smpl" && body.len() >= 60 {
            // the first loop: start, end (inclusive)
            let w = |k: usize| u32::from_le_bytes(body[4 * k..4 * k + 4].try_into().unwrap());
            if w(7) > 0 {
                looping = Some((w(11) as usize, w(12) as usize + 1));
            }
        } else if id == b"data" && frames.is_none() {
            if channels == 0 || rate == 0 {
                return None;
            }
            let ch = channels as usize;
            let sample = |i: usize| -> f32 {
                match bits {
                    8 => (body.get(i).copied().unwrap_or(128) as f32 - 128.0) / 128.0,
                    _ => {
                        let o = i * 2;
                        match body.get(o..o + 2) {
                            Some(s) => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
                            None => 0.0,
                        }
                    }
                }
            };
            let per = if bits == 8 { 1 } else { 2 };
            let count = body.len() / per / ch;
            frames = Some(
                (0..count)
                    .map(|f| {
                        let l = sample(f * ch);
                        let r = if ch > 1 { sample(f * ch + 1) } else { l };
                        [l, r]
                    })
                    .collect::<Vec<_>>(),
            );
        }
        pos += 8 + n + (n & 1);
    }
    let frames = frames?;
    let looping = looping.filter(|&(a, b)| a < b && b <= frames.len());
    Some(Clip {
        rate,
        frames,
        looping,
    })
}

/// Where clips come from without a folder: a WAV's bytes by name, or None while they are not
/// there (yet: the web build reads them from the browser's storage after starting).
pub type ClipReader = Box<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// A clip by import and name (both imports have e.g. a `SE_Select`).
type ClipKey = (UiKind, String);

struct Voice {
    clip: Arc<Clip>,
    pos: f64,
    step: f64,
    gain: f32,
    /// Output frames to wait before starting.
    delay: u64,
}

/// The menu music: one clip, looping or not, fading out when asked.
struct Music {
    clip: Arc<Clip>,
    pos: f64,
    step: f64,
    gain: f32,
    /// Output frames left of the fade-out, and its length.
    fade: Option<(u64, u64)>,
}

/// The UI sound mixer, shared between the game thread (`play`) and the audio callback
/// (`mix`).
pub struct UiMixer {
    rate: u32,
    voices: Mutex<Vec<Voice>>,
    music: Mutex<Option<Music>>,
    clips: Mutex<HashMap<ClipKey, Option<Arc<Clip>>>>,
    dir: Option<std::path::PathBuf>,
    smm1_dir: Option<std::path::PathBuf>,
    reader: Option<ClipReader>,
    /// Master × effects volume.
    pub volume: f32,
}

impl UiMixer {
    /// Clips are read from `dir` (`<ui import>/ui/sound`) on first use.
    pub fn new(dir: Option<std::path::PathBuf>, rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            voices: Mutex::new(Vec::new()),
            music: Mutex::new(None),
            clips: Mutex::new(HashMap::new()),
            dir,
            smm1_dir: None,
            reader: None,
            volume: 1.0,
        }
    }

    /// The SMM1 import's clips come from `dir` (`<smm1 import>/ui/sound`).
    pub fn with_smm1(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.smm1_dir = dir;
        self
    }

    /// Clips from `reader` (asked again until it has them).
    pub fn with_reader(reader: ClipReader, rate: u32) -> Self {
        Self {
            reader: Some(reader),
            ..Self::new(None, rate)
        }
    }

    fn clip(&self, ui: UiKind, name: &str) -> Option<Arc<Clip>> {
        let mut clips = self.clips.lock().ok()?;
        let k = (ui, name.to_string());
        if let Some(c) = clips.get(&k) {
            return c.clone();
        }
        if name.contains(['/', '\\']) || name.starts_with('.') {
            return None;
        }
        let dir = match ui {
            UiKind::Smm2 => &self.dir,
            UiKind::Smm1 => &self.smm1_dir,
        };
        let bytes = match (dir, &self.reader, ui) {
            (Some(dir), _, _) => std::fs::read(dir.join(format!("{name}.wav"))).ok(),
            // not there yet: ask again next time (the reader has the SMM2 import's only)
            (None, Some(read), UiKind::Smm2) => Some(read(name)?),
            _ => None,
        };
        let clip = bytes.and_then(|b| parse_wav(&b)).map(Arc::new);
        clips.insert(k, clip.clone());
        clip
    }

    /// Start `cue` (its delay counts from now).
    pub fn play(&self, cue: &mmk_front::SoundCue) {
        use mmk_front::CueKind;
        if std::env::var("MMK_DEBUG_SOUND").as_deref() == Ok("1") {
            crate::app::log(&format!(
                "UISound {:?} {} volume {} delay {}",
                cue.kind, cue.name, cue.volume, cue.delay
            ));
        }
        if cue.kind == CueKind::StopMusic {
            let frames = (cue.delay.max(0.0) as f64 / 60.0 * self.rate as f64) as u64;
            if let Ok(mut m) = self.music.lock() {
                match (frames, m.as_mut()) {
                    (0, _) => *m = None,
                    (n, Some(music)) if music.fade.is_none() => music.fade = Some((n, n)),
                    _ => {}
                }
            }
            return;
        }
        let Some(clip) = self.clip(cue.ui, &cue.name) else {
            return;
        };
        if matches!(cue.kind, CueKind::Music | CueKind::MusicTrack) {
            let step = clip.rate as f64 / self.rate as f64;
            if let Ok(mut m) = self.music.lock() {
                // another track of the playing stream carries on where it is
                let pos = match (cue.kind, m.as_ref()) {
                    (CueKind::MusicTrack, Some(old)) if old.fade.is_none() => {
                        old.pos * clip.rate as f64 / old.clip.rate as f64
                    }
                    _ => 0.0,
                };
                *m = Some(Music {
                    clip,
                    pos,
                    step,
                    gain: cue.volume * self.volume,
                    fade: None,
                });
            }
            return;
        }
        let step = clip.rate as f64 / self.rate as f64;
        let delay = (cue.delay.max(0.0) as f64 / 60.0 * self.rate as f64) as u64;
        if let Ok(mut v) = self.voices.lock() {
            v.push(Voice {
                clip,
                pos: 0.0,
                step,
                gain: cue.volume * self.volume,
                delay,
            });
        }
    }

    /// Add the voices into interleaved stereo `out`.
    pub fn mix(&self, out: &mut [f32]) {
        let Ok(mut voices) = self.voices.lock() else {
            return;
        };
        for v in voices.iter_mut() {
            for frame in out.as_chunks_mut::<2>().0 {
                if v.delay > 0 {
                    v.delay -= 1;
                    continue;
                }
                let i = v.pos as usize;
                let Some(a) = v.clip.frames.get(i) else {
                    break;
                };
                let b = v.clip.frames.get(i + 1).unwrap_or(a);
                let t = (v.pos - i as f64) as f32;
                frame[0] += (a[0] + (b[0] - a[0]) * t) * v.gain;
                frame[1] += (a[1] + (b[1] - a[1]) * t) * v.gain;
                v.pos += v.step;
            }
        }
        voices.retain(|v| v.delay > 0 || (v.pos as usize) < v.clip.frames.len());
        drop(voices);
        if let Ok(mut music) = self.music.lock() {
            let mut done = false;
            if let Some(m) = music.as_mut() {
                for frame in out.as_chunks_mut::<2>().0 {
                    if let Some((end_start, end)) = m.clip.looping
                        && m.pos >= end as f64
                    {
                        m.pos -= (end - end_start) as f64;
                    }
                    let i = m.pos as usize;
                    let Some(a) = m.clip.frames.get(i) else {
                        done = true;
                        break;
                    };
                    let b = m.clip.frames.get(i + 1).unwrap_or(a);
                    let t = (m.pos - i as f64) as f32;
                    let mut g = m.gain;
                    if let Some((left, total)) = &mut m.fade {
                        if *left == 0 {
                            done = true;
                            break;
                        }
                        g *= *left as f32 / *total as f32;
                        *left -= 1;
                    }
                    frame[0] += (a[0] + (b[0] - a[0]) * t) * g;
                    frame[1] += (a[1] + (b[1] - a[1]) * t) * g;
                    m.pos += m.step;
                }
            }
            if done {
                *music = None;
            }
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }
}

/// A running output stream carrying the game's sound (when the asset cache has a sound bank)
/// and the UI sounds.
pub struct FrontAudio {
    _stream: cpal::Stream,
    pub game: Option<Arc<mmk_host::audio::AudioOut>>,
    pub ui: Arc<UiMixer>,
}

/// Open the default output with the game's `bank` (optional) and the UI sounds of `ui_dir`
/// (the SMM2 import's) and `smm1_dir` (the SMM1 import's).
pub fn start(
    bank: Option<mmk_audio::Bank>,
    music: Option<mmk_host::audio::MusicReader>,
    ui_dir: Option<std::path::PathBuf>,
    smm1_dir: Option<std::path::PathBuf>,
) -> Result<FrontAudio, String> {
    type Slot = Arc<std::sync::OnceLock<(Option<Arc<mmk_host::audio::AudioOut>>, Arc<UiMixer>)>>;
    let slot: Slot = Arc::default();
    let feed = slot.clone();
    let output = crate::audio_out::open(move |buf| match feed.get() {
        Some((game, ui)) => {
            match game {
                Some(g) => g.render(buf),
                None => buf.fill(0.0),
            }
            ui.mix(buf);
            for s in buf.iter_mut() {
                *s = s.clamp(-1.0, 1.0);
            }
        }
        None => buf.fill(0.0),
    })?;
    let game = bank.map(|b| {
        let mut out = mmk_host::audio::AudioOut::with_rate(b, output.rate);
        if let Some(m) = music {
            out = out.with_music(m);
        }
        Arc::new(out)
    });
    let ui = Arc::new(UiMixer::new(ui_dir, output.rate).with_smm1(smm1_dir));
    let _ = slot.set((game.clone(), ui.clone()));
    Ok(FrontAudio {
        _stream: output.stream,
        game,
        ui,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * 2).to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend((samples.len() as u32 * 2).to_le_bytes());
        for s in samples {
            b.extend(s.to_le_bytes());
        }
        b
    }

    #[test]
    fn clips_mix_with_gain_and_delay() {
        let c = parse_wav(&wav(100, &[16384, 16384, 16384, 16384])).unwrap();
        assert_eq!((c.rate, c.frames.len(), c.looping), (100, 4, None));
        assert!((c.frames[0][1] - 0.5).abs() < 1e-6);
        let m = UiMixer::new(None, 100);
        m.voices.lock().unwrap().push(Voice {
            clip: Arc::new(c),
            pos: 0.0,
            step: 1.0,
            gain: 0.5,
            delay: 2,
        });
        let mut out = vec![0.0f32; 16];
        m.mix(&mut out);
        assert_eq!(out[0], 0.0);
        assert!((out[4] - 0.25).abs() < 1e-6);
        assert_eq!(out[12], 0.0);
        assert!(m.voices.lock().unwrap().is_empty());
    }

    fn with_loop(mut w: Vec<u8>, start: u32, end: u32) -> Vec<u8> {
        w.extend(b"smpl");
        w.extend(60u32.to_le_bytes());
        for v in [0, 0, 0, 60, 0, 0, 0, 1, 0, 0, 0, start, end, 0, 0u32] {
            w.extend(v.to_le_bytes());
        }
        let n = (w.len() - 8) as u32;
        w[4..8].copy_from_slice(&n.to_le_bytes());
        w
    }

    /// The menu music loops between its `smpl` points, switches tracks in place and fades out.
    #[test]
    fn music_loops_switches_and_fades() {
        let tmp = std::env::temp_dir().join(format!("mmk-ui-music-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // frames 0..3 then the loop 2..=3 (values 1000, 2000, 3000, 4000)
        let a = with_loop(wav(100, &[1000, 2000, 3000, 4000]), 2, 3);
        std::fs::write(tmp.join("A.wav"), &a).unwrap();
        std::fs::write(tmp.join("B.wav"), wav(100, &[-1000; 8])).unwrap();
        assert_eq!(parse_wav(&a).unwrap().looping, Some((2, 4)));
        let m = UiMixer::new(Some(tmp.clone()), 100);
        let cue = |name: &str, kind| mmk_front::SoundCue {
            name: name.into(),
            volume: 1.0,
            delay: 0.0,
            ui: UiKind::Smm2,
            kind,
        };
        m.play(&cue("A", mmk_front::CueKind::Music));
        let mut out = vec![0.0f32; 16];
        m.mix(&mut out);
        let l: Vec<i32> = out
            .iter()
            .step_by(2)
            .map(|x| (x * 32768.0).round() as i32)
            .collect();
        assert_eq!(l, [1000, 2000, 3000, 4000, 3000, 4000, 3000, 4000]);
        // another track at the same place, then a fade over 3 frames (at 60 Hz: 5 output
        // frames at 100 Hz)
        m.play(&cue("B", mmk_front::CueKind::MusicTrack));
        assert_eq!(m.music.lock().unwrap().as_ref().unwrap().pos, 4.0);
        m.play(&mmk_front::SoundCue::stop_music(3.0, UiKind::Smm2));
        let mut out = vec![0.0f32; 16];
        m.mix(&mut out);
        assert!(m.music.lock().unwrap().is_none(), "played out");
        assert!(out[0] < 0.0 && out[8] == 0.0, "{out:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// An SMM1 cue plays the SMM1 import's clip of that name, an SMM2 cue the SMM2 one's.
    #[test]
    fn cues_play_their_own_imports_clips() {
        let tmp = std::env::temp_dir().join(format!("mmk-ui-mixer-{}", std::process::id()));
        let (two, one) = (tmp.join("smm2"), tmp.join("smm1"));
        std::fs::create_dir_all(&two).unwrap();
        std::fs::create_dir_all(&one).unwrap();
        std::fs::write(two.join("SE_A.wav"), wav(100, &[1000; 3])).unwrap();
        std::fs::write(one.join("SE_A.wav"), wav(100, &[2000; 5])).unwrap();
        let m = UiMixer::new(Some(two), 100).with_smm1(Some(one));
        let cue = |ui| mmk_front::SoundCue {
            name: "SE_A".into(),
            volume: 1.0,
            delay: 0.0,
            ui,
            kind: mmk_front::CueKind::Sound,
        };
        m.play(&cue(UiKind::Smm1));
        m.play(&cue(UiKind::Smm2));
        let lens: Vec<usize> = m
            .voices
            .lock()
            .unwrap()
            .iter()
            .map(|v| v.clip.frames.len())
            .collect();
        assert_eq!(lens, [5, 3]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

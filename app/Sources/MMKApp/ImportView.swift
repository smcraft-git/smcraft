#if os(macOS)
import AppKit
import SwiftUI

/// First launch (no asset cache found), or "Import game data…" from the library: point the app at
/// the user's own game files (the NSP, the update NSP and prod.keys; or extracted folders) and run
/// the importer, or continue with placeholder art. (iOS has its own screen, CacheSetup.swift: the
/// importer runs on the Mac.)
struct ImportView: View {
    @EnvironmentObject private var model: AppModel

    @State private var fromFolders = false
    @State private var nsp: URL?
    @State private var updateNsp: URL?
    @State private var keys: URL?
    @State private var romfs: URL?
    @State private var exefs: URL?
    @State private var updateRomfs: URL?
    @State private var updateExefs: URL?
    @State private var running = false
    @State private var stage = ""
    @State private var progress: Double = 0
    @State private var error: String?
    @State private var importer: Importer?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(spacing: 14) {
                    Image(nsImage: NSApplication.shared.applicationIconImage)
                        .resizable().frame(width: 56, height: 56)
                    VStack(alignment: .leading) {
                        Text("Set up game data").font(.largeTitle.weight(.bold))
                        Text("\(AppIdentity.name) plays courses with art and data from your own copy of the game.")
                            .foregroundStyle(.secondary)
                    }
                }

                GroupBox {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("What you need").font(.headline)
                        if fromFolders {
                            Text("The **RomFS** and **ExeFS** folders extracted from your own dump of the Switch game (for example with an emulator's “Extract data” command). Version 3.0.3 is recommended: either point at a RomFS/ExeFS that already has the update applied, or add the update's folders below.")
                        } else {
                            Text("Your own dump of the Switch game as **NSP** files (the game, and the **3.0.3 update** for every course part and menu) and your console's **prod.keys**. The files are read where they are; nothing is extracted.")
                            Text("This is needed once. Afterwards \(AppIdentity.name) runs from the imported data alone: the NSP files and prod.keys can be put away. The keys are only used while importing and are never saved.")
                        }
                        Text("Nothing is downloaded and nothing leaves this Mac. The importer converts what the engine needs into \(AppPaths.assetsRoot.path).")
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
                }

                Picker("Import from", selection: $fromFolders) {
                    Text("NSP files").tag(false)
                    Text("Extracted folders").tag(true)
                }
                .pickerStyle(.segmented)
                .frame(maxWidth: 320)
                .disabled(running)

                VStack(spacing: 10) {
                    if fromFolders {
                        FolderRow(title: "RomFS", hint: "contains Pack/, Course/, Model/ …", url: $romfs,
                                  check: { looksLikeRomfs($0) ? nil : "No Pack/Static.pack or Course/ folder here" })
                        FolderRow(title: "ExeFS", hint: "contains main", url: $exefs,
                                  check: { hasMain($0) ? nil : "No file named main here" })
                        FolderRow(title: "Update RomFS", hint: "optional", url: $updateRomfs, check: { _ in nil })
                        FolderRow(title: "Update ExeFS", hint: "optional", url: $updateExefs,
                                  check: { hasMain($0) ? nil : "No file named main here" })
                    } else {
                        FolderRow(title: "Game NSP", hint: "the base game, version 1.0.0", url: $nsp,
                                  files: true, check: { isNsp($0) ? nil : "This is not an NSP file" })
                        FolderRow(title: "Update NSP", hint: "version 3.0.3 (recommended)", url: $updateNsp,
                                  files: true, check: { isNsp($0) ? nil : "This is not an NSP file" })
                        FolderRow(title: "prod.keys", hint: "your console's keys", url: $keys, files: true,
                                  check: { looksLikeKeys($0) ? nil : "Usually named prod.keys" })
                    }
                }
                .disabled(running)

                if running {
                    VStack(alignment: .leading, spacing: 4) {
                        ProgressView(value: progress)
                        Text(stage).font(.caption).foregroundStyle(.secondary)
                    }
                }
                if let error {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                        .textSelection(.enabled)
                }

                HStack {
                    Button("Use placeholder art") { model.usePlaceholder() }
                        .help("Play without game data: simple coloured shapes, unencrypted courses only")
                        .disabled(running)
                    if model.assets != nil {
                        Button("Back to library") { model.showLibrary() }.disabled(running)
                    }
                    Spacer()
                    Button(running ? "Importing…" : "Import") { start() }
                        .buttonStyle(.borderedProminent)
                        .keyboardShortcut(.defaultAction)
                        .disabled(running || !canStart)
                }
            }
            .padding(28)
            .frame(maxWidth: 720)
            .frame(maxWidth: .infinity)
        }
    }

    private var canStart: Bool {
        if !fromFolders {
            guard let nsp, keys != nil else { return false }
            return isNsp(nsp) && updateNsp.map(isNsp) != false
        }
        guard let romfs, let exefs else { return false }
        return (looksLikeRomfs(romfs) || updateRomfs != nil) && (hasMain(exefs) || updateExefs.map(hasMain) == true)
    }

    private func looksLikeRomfs(_ u: URL) -> Bool {
        let fm = FileManager.default
        return fm.fileExists(atPath: u.appendingPathComponent("Pack/Static.pack").path)
            || fm.fileExists(atPath: u.appendingPathComponent("Course").path)
    }

    private func hasMain(_ u: URL) -> Bool {
        FileManager.default.fileExists(atPath: u.appendingPathComponent("main").path)
    }

    /// An NSP starts with the PFS0 magic.
    private func isNsp(_ u: URL) -> Bool {
        guard let h = try? FileHandle(forReadingFrom: u) else { return false }
        defer { try? h.close() }
        return (try? h.read(upToCount: 4)) == Data("PFS0".utf8)
    }

    /// The keys file is not opened here: only the importer reads it, once.
    private func looksLikeKeys(_ u: URL) -> Bool {
        u.pathExtension.lowercased() == "keys"
    }

    private static let stageNames = [
        "open": "Opening the game files", "identify": "Identifying the game version",
        "keys": "Finding the course key table", "tables": "Converting game data tables",
        "textures": "Converting textures", "audio": "Converting sounds",
        "sound users": "Reading the sound database", "sound waves": "Converting sounds",
        "effect sets": "Converting effects", "effect links": "Converting effects",
        "effect textures": "Converting effect textures", "modes": "Reading Story Mode data",
        "layouts": "Converting menu layouts", "fonts": "Converting fonts",
        "scalable fonts": "Converting fonts", "messages": "Converting text",
        "ui sounds": "Converting menu sounds",
    ]

    /// The stages in the order the importer runs them (the UI import identifies the game again).
    private static let order = [
        "open", "identify", "keys", "tables", "textures", "audio", "sound users", "sound waves",
        "effect sets", "effect links", "effect textures", "modes", "identify", "layouts", "fonts",
        "scalable fonts", "messages", "ui sounds",
    ]

    private func start() {
        let source: Importer.Source
        if fromFolders {
            guard let romfs, let exefs else { return }
            source = .folders(romfs: romfs, exefs: exefs, updateRomfs: updateRomfs, updateExefs: updateExefs)
        } else {
            guard let nsp, let keys else { return }
            source = .nsp(base: nsp, update: updateNsp, keys: keys)
        }
        running = true
        error = nil
        progress = 0
        stage = "Starting…"
        let order = fromFolders ? ["identify", "keys", "tables", "textures"] : Self.order
        var at = 0
        let imp = Importer { s, done, total in
            // the first match at or after the current stage (names repeat)
            if let i = order[at...].firstIndex(of: s) { at = i }
            let frac = total > 0 ? Double(done) / Double(total) : 0
            progress = (Double(at) + frac) / Double(order.count)
            stage = "\(Self.stageNames[s] ?? s)… \(done)/\(total)"
        }
        importer = imp
        imp.run(source, outRoot: AppPaths.assetsRoot) { err in
            running = false
            importer = nil
            if let err { error = "Import failed: \(err)" } else { model.importFinished() }
        }
    }
}

/// One file or folder to pick.
private struct FolderRow: View {
    let title: String
    let hint: String
    @Binding var url: URL?
    var files = false
    let check: (URL) -> String?

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title).fontWeight(.semibold).frame(width: 110, alignment: .leading)
            VStack(alignment: .leading, spacing: 2) {
                Text(url?.path ?? hint)
                    .foregroundStyle(url == nil ? .secondary : .primary)
                    .lineLimit(1).truncationMode(.middle)
                if let url, let problem = check(url) {
                    Text(problem).font(.caption).foregroundStyle(.orange)
                }
            }
            Spacer()
            if url != nil { Button("Clear") { url = nil }.buttonStyle(.borderless) }
            Button("Choose…") { choose() }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.08)))
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.title = files ? "Choose the \(title) file" : "Choose the \(title) folder"
        panel.canChooseDirectories = !files
        panel.canChooseFiles = files
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK { url = panel.url }
    }
}
#endif

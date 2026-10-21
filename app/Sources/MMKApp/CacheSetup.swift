#if os(iOS)
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// iOS first launch (no asset cache found), or "Game data…" from the library. The player imports
/// their own dump here once: the SMM2 NSP, the 3.0.3 update's NSP and prod.keys, picked in Files
/// (read in place) or copied into On My iPad › SMCraft › import; the importer (`mmk_import_nsp`)
/// writes the asset pack and the game never reads the dump or the keys again. Or bring a cache
/// made on a Mac over (copied by `CacheCopy`), or continue with placeholder art.
struct ImportView: View {
    @EnvironmentObject private var model: AppModel

    @State private var picking = false
    @State private var pickingDump = false
    @State private var copying = false
    @State private var importing = false
    @State private var progress = ""
    @State private var error: String?

    private var device: String { UIDevice.current.model }   // "iPad" / "iPhone"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(spacing: 14) {
                    Image(systemName: "gamecontroller.fill").font(.system(size: 40)).foregroundStyle(.tint)
                    VStack(alignment: .leading) {
                        Text("Set up game data").font(.largeTitle.weight(.bold))
                        Text("\(AppIdentity.name) plays courses with art and data from your own copy of the game.")
                            .foregroundStyle(.secondary)
                    }
                }

                GroupBox {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Import your own game files").font(.headline)
                        Text("Choose your Super Mario Maker 2 NSP, the 3.0.3 update's NSP and your prod.keys, all three at once. Or copy them with the Files app into **On My \(device) › \(AppIdentity.name) › import** and tap Look again.")
                        Text("They are read once, here on this \(device), into the game's own data. After that \(AppIdentity.name) never needs them again and you can delete them.")
                            .foregroundStyle(.secondary)
                        Button("Choose game files…") { pickingDump = true }
                            .buttonStyle(.borderedProminent)
                            .disabled(importing || copying)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
                }
                .fileImporter(isPresented: $pickingDump, allowedContentTypes: [.item],
                              allowsMultipleSelection: true) { result in
                    switch result {
                    case .success(let urls): importDump(urls, scoped: true)
                    case .failure(let e): error = e.localizedDescription
                    }
                }

                if importing { ProgressView(progress.isEmpty ? "Importing…" : progress) }

                GroupBox {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Or bring an import over from your Mac").font(.headline)
                        Text("Import your dump on a Mac first, with the macOS app or `mmk import`. That makes a folder such as **smm2-3.0.3**, and optionally **smm1-…** and **smm2-ui-…** for the menus.")
                        Text("Then copy those folders with the Files app into **On My \(device) › \(AppIdentity.name) › assets**, or choose them below from anywhere Files can reach (iCloud Drive, a USB drive, AirDrop) and they are copied in. Choosing the folder that holds several caches copies all of them.")
                        Text("Nothing is downloaded and nothing leaves this \(device). Menu layouts (smm1-…, smm2-ui-…) are used from the next launch.")
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
                }

                if copying { ProgressView("Copying…") }
                if let error {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                        .textSelection(.enabled)
                }

                ViewThatFits {
                    HStack { buttons }
                    VStack(alignment: .leading, spacing: 12) { buttons }
                }
                .disabled(copying || importing)
            }
            .padding(28)
            .frame(maxWidth: 720)
            .frame(maxWidth: .infinity)
        }
        .onAppear {
            // game files already in the import folder: start at once
            let files = (try? FileManager.default.contentsOfDirectory(at: dropFolder, includingPropertiesForKeys: nil)) ?? []
            if !importing && files.contains(where: { $0.pathExtension.lowercased() == "nsp" }) { lookAgain() }
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
            switch result {
            case .success(let url): copy(url)
            case .failure(let e): error = e.localizedDescription
            }
        }
    }

    @ViewBuilder private var buttons: some View {
        Button("Choose cache folder…") { picking = true }
        Button("Look again") { lookAgain() }
        Button("Use placeholder art") { model.usePlaceholder() }
        if model.assets != nil {
            Button("Back to library") { model.showLibrary() }
        }
    }

    /// The import folder in Documents (the Files app shows it): game files there are imported,
    /// else an asset cache copied into assets is looked for.
    private var dropFolder: URL { AppPaths.support.appendingPathComponent("import", isDirectory: true) }

    private func lookAgain() {
        try? FileManager.default.createDirectory(at: dropFolder, withIntermediateDirectories: true)
        let files = (try? FileManager.default.contentsOfDirectory(at: dropFolder, includingPropertiesForKeys: nil)) ?? []
        if files.contains(where: { $0.pathExtension.lowercased() == "nsp" }) {
            importDump(files, scoped: false)
        } else {
            model.importFinished()
        }
    }

    /// Import from the picked (security-scoped) or dropped files: one or two NSPs and the keys.
    private func importDump(_ urls: [URL], scoped: Bool) {
        let access = scoped ? urls.filter { $0.startAccessingSecurityScopedResource() } : []
        let release = { access.forEach { $0.stopAccessingSecurityScopedResource() } }
        let nsps = urls.filter { $0.pathExtension.lowercased() == "nsp" }.sorted { $0.path < $1.path }
        let keys = urls.first { $0.lastPathComponent.lowercased() == "prod.keys" }
        guard let keys, (1...2).contains(nsps.count) else {
            release()
            error = "Choose the game's NSP (and the update's) together with prod.keys."
            return
        }
        error = nil
        importing = true
        // which NSP is the base game isn't known from the names: the other order if one fails
        let orders: [(URL, URL?)] = nsps.count == 2 ? [(nsps[0], nsps[1]), (nsps[1], nsps[0])] : [(nsps[0], nil)]
        run(orders[...], keys: keys) { err in
            release()
            importing = false
            if let err { error = err } else { model.importFinished() }
        }
    }

    private func run(_ orders: ArraySlice<(URL, URL?)>, keys: URL, completion: @escaping (String?) -> Void) {
        guard let (base, update) = orders.first else { return completion("Nothing to import.") }
        let importer = Importer { stage, done, total in
            progress = total > 0 ? "Importing: \(stage) \(done * 100 / total)%" : "Importing: \(stage)"
        }
        importer.run(.nsp(base: base, update: update, keys: keys), outRoot: AppPaths.assetsRoot) { err in
            if err != nil && orders.count > 1 {
                run(orders.dropFirst(), keys: keys, completion: completion)
            } else {
                completion(err)
            }
        }
    }

    private func copy(_ url: URL) {
        copying = true
        error = nil
        CacheCopy.run(url) { err in
            copying = false
            if let err { error = err } else { model.importFinished() }
        }
    }
}

/// Copies asset caches (folders with a manifest.json) into `AppPaths.assetsRoot`: the picked
/// folder itself, or the caches directly inside it (the Mac's whole `cache/` folder). An older
/// copy of the same cache is replaced.
enum CacheCopy {
    static func run(_ src: URL, completion: @escaping (String?) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            // a folder picked in Files is readable only inside its security scope
            let scoped = src.startAccessingSecurityScopedResource()
            defer { if scoped { src.stopAccessingSecurityScopedResource() } }
            let fm = FileManager.default
            let children = (try? fm.contentsOfDirectory(at: src, includingPropertiesForKeys: nil)) ?? []
            let caches = AppPaths.isAssetCache(src) ? [src] : children.filter(AppPaths.isAssetCache)
            var err: String? = caches.isEmpty
                ? "\(src.lastPathComponent) is not an asset cache (no manifest.json in it or in its folders)." : nil
            for cache in caches {
                let dest = AppPaths.assetsRoot.appendingPathComponent(cache.lastPathComponent, isDirectory: true)
                // already in place (picked from the app's own assets folder)
                if cache.standardizedFileURL.resolvingSymlinksInPath() == dest.standardizedFileURL.resolvingSymlinksInPath() { continue }
                do {
                    try fm.createDirectory(at: AppPaths.assetsRoot, withIntermediateDirectories: true)
                    if fm.fileExists(atPath: dest.path) { try fm.removeItem(at: dest) }
                    try fm.copyItem(at: cache, to: dest)
                } catch {
                    err = "Copying \(cache.lastPathComponent) failed: \(error.localizedDescription)"
                }
            }
            DispatchQueue.main.async { completion(err) }
        }
    }
}
#endif

//! Import once, then ROM-free (PROGRESS.md G1): after `mmk import --nsp` the game needs neither
//! the dump nor `prod.keys`.
//!
//! - The sources of everything that runs the game (every crate but the importer and the `mmk`
//!   tool, and the Swift app) have no code that opens an NSP, NCA, ticket or key file. Only the
//!   setup screens start an import (`mmk_import::run_nsp`, or the web import's
//!   `mmk_import::open_nsps` over the picked files), and even they leave the keys to the importer.
//! - A built-in course plays from an imported pack alone (skipped without one, as in CI:
//!   `MMK_ASSETS=<pack dir>`, else the newest import in the data folder, else
//!   ~/smm-work/cache/smm2-3.0.3). This test links no importer code at all.

use std::path::{Path, PathBuf};

/// The repository root (this crate is `core/crates/host`).
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Files that start an import (the first-launch setup screens and their bindings).
const SETUP: &[&str] = &[
    "core/crates/ffi/src/import.rs",
    "core/crates/shell/src/web_import.rs",
    "core/crates/shell/src/setup.rs",
    "core/crates/shell/web/index.html",
    "core/crates/shell/web/mmk-web.js",
    "core/crates/shell/web/mmk-import-worker.js",
    "app/Sources/MMKApp/ImportView.swift",
    "app/Sources/MMKApp/CacheSetup.swift",
    "app/Sources/MMKApp/Bridge.swift",
];

/// Key handling: only in the importer, setup screens included (they hand it the file or its
/// text, the importer reads it).
const KEYS: &[&str] = &[
    "Keys::",
    "header_key",
    "titlekek",
    "key_area_key",
    ".nca\"",
    ".tik\"",
    "FileStorage",
];

/// The dump readers and starting an import: only in the setup screens.
const IMPORT: &[&str] = &["mmk_import::", "nsp::", "DumpVfs", "prod.keys", ".nsp"];

/// `line` mentions `token`: a token that starts with a name character must start a name there
/// (`Keys::` is the importer's key file, `CourseKeys::` the course key table from the pack).
fn mentions(line: &str, token: &str) -> bool {
    let name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let starts_name = token.chars().next().is_some_and(name);
    line.match_indices(token)
        .any(|(i, _)| !starts_name || !line[..i].chars().next_back().is_some_and(name))
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n != "target" && n != ".build")
            {
                sources(&p, out);
            }
        } else if p
            .extension()
            .is_some_and(|x| ["rs", "swift", "js", "html"].iter().any(|e| x == *e))
        {
            out.push(p);
        }
    }
}

/// The runtime's source files: every crate except the importer and the `mmk` tool, and the app.
fn runtime_sources() -> Vec<PathBuf> {
    let root = repo();
    let mut files = Vec::new();
    for e in std::fs::read_dir(root.join("core/crates"))
        .unwrap()
        .flatten()
    {
        let name = e.file_name();
        if name != "import" && name != "cli" {
            sources(&e.path(), &mut files);
        }
    }
    sources(&root.join("app/Sources"), &mut files);
    files
}

#[test]
fn runtime_never_opens_the_dump_or_keys() {
    let root = repo();
    let files = runtime_sources();
    assert!(files.len() > 100, "found only {} sources", files.len());
    let mut hits = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "core/crates/host/tests/rom_free.rs" {
            continue; // this file's own lists
        }
        let setup = SETUP.contains(&rel.as_str());
        let text = std::fs::read_to_string(f).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue; // doc comments name the importer's modules
            }
            let lower = code.to_ascii_lowercase();
            let forbidden = KEYS
                .iter()
                .chain(if setup { &[][..] } else { IMPORT })
                .find(|t| {
                    // type names (capitals) as written; file names and the rest in any case
                    if t.chars().any(|c| c.is_ascii_uppercase()) {
                        mentions(code, t)
                    } else {
                        mentions(&lower, t)
                    }
                });
            if let Some(t) = forbidden {
                hits.push(format!("{rel}:{}: {t}", n + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "dump or key access outside the importer:\n{}",
        hits.join("\n")
    );
}

#[test]
fn only_the_setup_crates_link_the_importer() {
    let root = repo();
    for e in std::fs::read_dir(root.join("core/crates"))
        .unwrap()
        .flatten()
    {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(toml) = std::fs::read_to_string(e.path().join("Cargo.toml")) else {
            continue;
        };
        let links = toml
            .lines()
            .any(|l| l.trim_start().starts_with("mmk-import"));
        let allowed = ["import", "cli", "ffi", "shell"].contains(&name.as_str());
        assert!(!links || allowed, "{name} depends on mmk-import");
    }
}

fn pack() -> Option<PathBuf> {
    std::env::var_os("MMK_ASSETS")
        .map(PathBuf::from)
        .or_else(mmk_host::paths::game_cache)
        .filter(|p| p.join("manifest.json").is_file() && p.join("courses").is_dir())
}

#[test]
fn a_course_plays_from_the_pack_alone() {
    let Some(root) = pack() else {
        eprintln!("skipped: no imported asset pack");
        return;
    };
    let bcd = std::fs::read(root.join("courses/quest_000.bcd")).unwrap();
    let course = mmk_host::course::decode(&bcd, || {
        std::fs::read(root.join(mmk_host::course::KEY_TABLE)).map_err(|e| e.to_string())
    })
    .expect("built-in course decrypts with the pack's key table");
    let mut world = mmk_sim::World::from_course(&course);
    let start = world.player.x;
    for _ in 0..240 {
        world.step(mmk_sim::Input {
            buttons: mmk_sim::input::RIGHT,
        });
    }
    assert!(world.player.x > start + 32.0, "Mario did not run right");
    let cache = mmk_assets::Cache::open(&root).expect("textures from the pack");
    assert!(!cache.textures.is_empty());
    let bank = mmk_audio::Bank::load_cache(&root).expect("sounds from the pack");
    assert!(bank.id("jump_small").is_some());
}

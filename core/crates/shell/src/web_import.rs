//! The game-data import in the browser (wasm32, run in a worker by `web/mmk-import-worker.js`):
//! the same importer as `mmk import` (`mmk-import`), reading the RomFS and ExeFS files the owner
//! picked and handing every output file to the page, which stores it in the browser's own
//! storage. Nothing leaves the browser.
//!
//! The picked files are either the extracted RomFS and ExeFS, or the dump itself: the NSP files
//! (the base game and its update, in any order) and `prod.keys`, read in place by the importer's
//! own readers ([`open_nsps`]) and served to the same import stages.
//!
//! Workers can read picked files synchronously (`FileReaderSync`), which the importer needs; they
//! can't wait for storage, so outputs go to the page as messages. The worker's
//! `mmkImport` object provides the calls below.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mmk_import::nsp::DumpVfs;
use mmk_import::nsp::storage::{Shared, Storage};
use mmk_import::vfs::Vfs;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The bytes of a picked file (`/romfs/...`, `/exefs/main`), or null.
    #[wasm_bindgen(js_namespace = mmkImport, js_name = read)]
    fn js_read(path: &str) -> JsValue;
    /// Names of the entries directly inside a picked directory.
    #[wasm_bindgen(js_namespace = mmkImport, js_name = list)]
    fn js_list(dir: &str) -> js_sys::Array;
    #[wasm_bindgen(js_namespace = mmkImport, js_name = isFile)]
    fn js_is_file(path: &str) -> bool;
    /// An output file, by its path in the asset cache (`textures/...`).
    #[wasm_bindgen(js_namespace = mmkImport, js_name = write)]
    fn js_write(path: &str, bytes: js_sys::Uint8Array);
    #[wasm_bindgen(js_namespace = mmkImport, js_name = progress)]
    fn js_progress(stage: &str, done: u32, total: u32);
    /// The size of picked NSP `index`.
    #[wasm_bindgen(js_namespace = mmkImport, js_name = nspSize)]
    fn js_nsp_size(index: u32) -> f64;
    /// `len` bytes of picked NSP `index` from `offset`.
    #[wasm_bindgen(js_namespace = mmkImport, js_name = nspRead)]
    fn js_nsp_read(index: u32, offset: f64, len: u32) -> js_sys::Uint8Array;
}

/// A picked NSP file, read range by range.
struct PickedNsp {
    index: u32,
    size: u64,
}

impl Storage for PickedNsp {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if offset + buf.len() as u64 > self.size {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let bytes = js_nsp_read(self.index, offset as f64, buf.len() as u32);
        if bytes.length() as usize != buf.len() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        bytes.copy_to(buf);
        Ok(())
    }
}

thread_local! {
    /// The dump [`open_nsps`] opened, read by the imports instead of picked RomFS / ExeFS files.
    static DUMP: RefCell<Option<Rc<dyn Vfs>>> = const { RefCell::new(None) };
}

/// The imports' file access: the opened dump (outputs to the page), else the picked folders.
fn input() -> Rc<dyn Vfs> {
    DUMP.with(|d| d.borrow().clone())
        .unwrap_or_else(|| Rc::new(WorkerFs))
}

/// Open the picked NSPs (`names[i]` is picked NSP `i`; the base game and optionally its update,
/// in any order) with the picked `prod.keys`' text; [`import_dump`] and [`import_ui`] then read
/// the dump. The keys are dropped before this returns.
#[wasm_bindgen]
pub fn open_nsps(names: js_sys::Array, keys: &str) -> Result<(), JsValue> {
    let nsps = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let storage: Shared = Arc::new(PickedNsp {
                index: i as u32,
                size: js_nsp_size(i as u32) as u64,
            });
            (
                name.as_string().unwrap_or_else(|| format!("NSP {}", i + 1)),
                storage,
            )
        })
        .collect();
    let dump = mmk_import::open_nsps(nsps, keys).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let fs: Rc<dyn Vfs> = Rc::new(DumpVfs::with_output(
        dump,
        Path::new("/"),
        Rc::new(WorkerFs),
    ));
    DUMP.with(|d| *d.borrow_mut() = Some(fs));
    Ok(())
}

/// Where the importer is told to put the cache; stripped (with the build folder) on output.
const OUT: &str = "/out";
/// Where the SMM2 UI importer is told to put its import; output paths there get [`UI_PREFIX`].
const UI_OUT: &str = "/ui-out";
const UI_PREFIX: &str = "@ui/";

struct WorkerFs;

fn path_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

impl Vfs for WorkerFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let v = js_read(&path_str(path));
        if v.is_null() || v.is_undefined() {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(js_sys::Uint8Array::new(&v).to_vec())
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        // the SMM2 UI import: /ui-out/<rel> → @ui/<rel> (the page keeps it apart)
        if let Ok(rel) = path.strip_prefix(UI_OUT) {
            js_write(
                &format!("{UI_PREFIX}{}", path_str(rel)),
                js_sys::Uint8Array::from(data),
            );
            return Ok(());
        }
        // /out/smm2-<build>/<rel> → <rel>
        let rel = path
            .strip_prefix(OUT)
            .ok()
            .and_then(|p| {
                p.iter()
                    .next()
                    .map(|build| p.strip_prefix(build).unwrap_or(p))
            })
            .ok_or_else(|| {
                io::Error::other(format!("output outside the cache: {}", path_str(path)))
            })?;
        js_write(&path_str(rel), js_sys::Uint8Array::from(data));
        Ok(())
    }
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(js_list(&path_str(dir))
            .iter()
            .filter_map(|n| n.as_string())
            .map(|n| dir.join(n))
            .collect())
    }
    fn is_file(&self, path: &Path) -> bool {
        js_is_file(&path_str(path))
    }
    fn remove_dir_all(&self, _dir: &Path) -> io::Result<()> {
        // the page imports into empty storage
        Ok(())
    }
}

/// Run the import over the worker's files (`/romfs/…`, `/exefs/main`). Returns a summary
/// (`{build, textures, sounds, courses}`).
#[wasm_bindgen]
pub fn import_dump() -> Result<JsValue, JsValue> {
    let cfg = mmk_import::ImportConfig {
        romfs: PathBuf::from("/romfs"),
        exefs: PathBuf::from("/exefs"),
        out_root: PathBuf::from(OUT),
    };
    let report = mmk_import::vfs::with_vfs(input(), || {
        mmk_import::run(&cfg, &mut |stage, done, total| {
            js_progress(stage, done as u32, total as u32)
        })
    })
    .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let summary = serde_json::json!({
        "build": report.build,
        "textures": report.textures,
        "sounds": report.sounds,
        "courses": report.builtin_courses_verified,
    });
    Ok(JsValue::from_str(&summary.to_string()))
}

/// Run the SMM2 UI import (`mmk import-smm2-ui`: the menus' layouts, fonts, messages and
/// sounds) over the same files; its outputs are written as `@ui/<path>`. Returns a summary
/// (`{build, layouts, textures, fonts}`).
#[wasm_bindgen]
pub fn import_ui() -> Result<JsValue, JsValue> {
    let cfg = mmk_import::smm2_ui::Smm2UiConfig {
        romfs: PathBuf::from("/romfs"),
        exefs: Some(PathBuf::from("/exefs")),
        out: PathBuf::from(UI_OUT),
        language: "USen".into(),
    };
    let report = mmk_import::vfs::with_vfs(input(), || {
        mmk_import::run_smm2_ui(&cfg, &mut |stage, done, total| {
            js_progress(&format!("menus: {stage}"), done as u32, total as u32)
        })
    })
    .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let summary = serde_json::json!({
        "build": report.build,
        "layouts": report.layouts,
        "textures": report.textures,
        "fonts": report.fonts,
    });
    Ok(JsValue::from_str(&summary.to_string()))
}

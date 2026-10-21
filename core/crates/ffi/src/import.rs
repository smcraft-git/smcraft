//! `mmk_import_nsp` / `mmk_import_run`: the first-launch importer (same code as `mmk import`) for
//! the app: from the player's NSP files and `prod.keys`, or from extracted RomFS / ExeFS folders.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;

use crate::{guard, set_error};

/// Progress callback: (user, stage, done, total). Called on the importing thread.
pub type MmkImportProgress = Option<
    unsafe extern "C" fn(user: *mut c_void, stage: *const c_char, done: usize, total: usize),
>;

pub use mmk_import::overlay::overlay_romfs;

fn opt_path(p: *const c_char) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// The C progress callback as the importer's.
fn progress_fn(progress: MmkImportProgress, user: *mut c_void) -> impl FnMut(&str, usize, usize) {
    move |stage: &str, done: usize, total: usize| {
        if let Some(f) = progress {
            let stage = CString::new(stage).unwrap_or_default();
            unsafe { f(user, stage.as_ptr(), done, total) };
        }
    }
}

/// Run every import stage (the game cache `<out_root>/smm2-<build>/` and the SMM2 UI
/// `<out_root>/smm2-ui-<build>/`) from the player's own base game NSP, optionally its update NSP,
/// and `prod.keys`, read in place. The keys are used while the dump is opened and never stored;
/// afterwards the game needs neither file. `language` may be NULL (`USen`). Blocks until done:
/// call it off the main thread. `progress` may be NULL. Returns false and sets the last error
/// (on this thread) on failure.
///
/// # Safety
/// Path arguments must be NUL-terminated strings (`update` and `language` may be NULL); `user`
/// is passed through to `progress` untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmk_import_nsp(
    base: *const c_char,
    update: *const c_char,
    keys: *const c_char,
    out_root: *const c_char,
    language: *const c_char,
    progress: MmkImportProgress,
    user: *mut c_void,
) -> bool {
    guard(false, || {
        let (Some(base), Some(keys), Some(out_root)) =
            (opt_path(base), opt_path(keys), opt_path(out_root))
        else {
            set_error("base, keys and out_root are required");
            return false;
        };
        let cfg = mmk_import::NspImportConfig {
            base,
            update: opt_path(update),
            keys,
            out_root,
            language: opt_path(language)
                .map_or_else(|| "USen".into(), |l| l.to_string_lossy().into_owned()),
        };
        match mmk_import::run_nsp(&cfg, &mut progress_fn(progress, user)) {
            Ok(_) => true,
            Err(e) => {
                set_error(e.to_string());
                false
            }
        }
    })
}

/// Run the importer on the user's own extracted RomFS + ExeFS (optionally with a separate update
/// RomFS / ExeFS laid over them) into `<out_root>/smm2-<build>/`. Blocks until done: call it off
/// the main thread. `progress` may be NULL. Returns false and sets the last error (on this
/// thread) on failure.
///
/// # Safety
/// Path arguments must be NUL-terminated strings (the update ones may be NULL); `user` is passed
/// through to `progress` untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmk_import_run(
    romfs: *const c_char,
    exefs: *const c_char,
    update_romfs: *const c_char,
    update_exefs: *const c_char,
    out_root: *const c_char,
    progress: MmkImportProgress,
    user: *mut c_void,
) -> bool {
    guard(false, || {
        let (Some(romfs), Some(exefs), Some(out_root)) =
            (opt_path(romfs), opt_path(exefs), opt_path(out_root))
        else {
            set_error("romfs, exefs and out_root are required");
            return false;
        };
        let update_romfs = opt_path(update_romfs);
        let exefs = opt_path(update_exefs).unwrap_or(exefs);

        let merge_dir = out_root.join(".romfs-overlay");
        let romfs = match &update_romfs {
            Some(update) => {
                if let Err(e) = overlay_romfs(&romfs, update, &merge_dir) {
                    set_error(format!("merging update RomFS: {e}"));
                    return false;
                }
                merge_dir.clone()
            }
            None => romfs,
        };
        let cfg = mmk_import::ImportConfig {
            romfs,
            exefs,
            out_root,
        };
        let result = mmk_import::run(&cfg, &mut progress_fn(progress, user));
        if update_romfs.is_some() {
            let _ = std::fs::remove_dir_all(&merge_dir);
        }
        match result {
            Ok(_) => true,
            Err(e) => {
                set_error(e.to_string());
                false
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_prefers_update_files() {
        let root = std::env::temp_dir().join(format!("mmk-overlay-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (base, upd, out) = (root.join("base"), root.join("upd"), root.join("out"));
        std::fs::create_dir_all(base.join("Pack")).unwrap();
        std::fs::create_dir_all(upd.join("Pack")).unwrap();
        std::fs::write(base.join("Pack/a"), "base-a").unwrap();
        std::fs::write(base.join("Pack/b"), "base-b").unwrap();
        std::fs::write(upd.join("Pack/b"), "upd-b").unwrap();
        std::fs::write(upd.join("new"), "upd-new").unwrap();
        overlay_romfs(&base, &upd, &out).unwrap();
        let read = |p: &str| std::fs::read_to_string(out.join(p)).unwrap();
        assert_eq!(read("Pack/a"), "base-a");
        assert_eq!(read("Pack/b"), "upd-b");
        assert_eq!(read("new"), "upd-new");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// End to end on the owner's dump (skipped unless the paths are given; CI has no game data):
    /// MMK_TEST_ROMFS, MMK_TEST_EXEFS [MMK_TEST_UPDATE_ROMFS, MMK_TEST_UPDATE_EXEFS].
    #[test]
    fn import_real_dump_when_available() {
        let var = |k: &str| std::env::var(k).ok().map(|s| CString::new(s).unwrap());
        let (Some(romfs), Some(exefs)) = (var("MMK_TEST_ROMFS"), var("MMK_TEST_EXEFS")) else {
            return;
        };
        let (urom, uexe) = (var("MMK_TEST_UPDATE_ROMFS"), var("MMK_TEST_UPDATE_EXEFS"));
        let out = std::env::temp_dir().join(format!("mmk-import-test-{}", std::process::id()));
        let out_c = CString::new(out.to_string_lossy().as_bytes()).unwrap();
        unsafe extern "C" fn cb(
            user: *mut c_void,
            stage: *const c_char,
            done: usize,
            total: usize,
        ) {
            let calls = unsafe { &mut *(user as *mut Vec<String>) };
            let s = unsafe { CStr::from_ptr(stage) }.to_string_lossy();
            calls.push(format!("{s} {done}/{total}"));
        }
        let mut calls: Vec<String> = Vec::new();
        let ptr = |c: &Option<CString>| c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
        let ok = unsafe {
            mmk_import_run(
                romfs.as_ptr(),
                exefs.as_ptr(),
                ptr(&urom),
                ptr(&uexe),
                out_c.as_ptr(),
                Some(cb),
                &mut calls as *mut Vec<String> as *mut c_void,
            )
        };
        assert!(
            ok,
            "{}",
            unsafe { CStr::from_ptr(crate::mmk_last_error()) }.to_string_lossy()
        );
        assert!(calls.iter().any(|c| c.starts_with("textures")));
        let dir = std::fs::read_dir(&out)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.join("manifest.json").is_file())
            .expect("cache written");
        assert!(dir.join("keys/course_table.bin").is_file());
        assert!(!out.join(".romfs-overlay").exists());
        eprintln!("imported into {}", dir.display());
        std::fs::remove_dir_all(&out).unwrap();
    }

    /// End to end from the owner's NSPs (skipped unless the paths are given; CI has no game
    /// data): MMK_TEST_NSP, MMK_TEST_PROD_KEYS [MMK_TEST_UPDATE_NSP]. Run it in release.
    #[test]
    fn import_real_nsp_when_available() {
        let var = |k: &str| std::env::var(k).ok().map(|s| CString::new(s).unwrap());
        let (Some(base), Some(keys)) = (var("MMK_TEST_NSP"), var("MMK_TEST_PROD_KEYS")) else {
            return;
        };
        let update = var("MMK_TEST_UPDATE_NSP");
        let out = std::env::temp_dir().join(format!("mmk-import-nsp-test-{}", std::process::id()));
        let out_c = CString::new(out.to_string_lossy().as_bytes()).unwrap();
        let ok = unsafe {
            mmk_import_nsp(
                base.as_ptr(),
                update.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
                keys.as_ptr(),
                out_c.as_ptr(),
                std::ptr::null(),
                None,
                std::ptr::null_mut(),
            )
        };
        assert!(
            ok,
            "{}",
            unsafe { CStr::from_ptr(crate::mmk_last_error()) }.to_string_lossy()
        );
        let names: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .filter(|e| e.path().join("manifest.json").is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.starts_with("smm2-ui-")), "{names:?}");
        assert!(
            names.iter().any(|n| !n.starts_with("smm2-ui-")),
            "{names:?}"
        );
        std::fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn nsp_import_reports_a_bad_keys_file() {
        let dir = std::env::temp_dir().join(format!("mmk-nsp-ffi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (nsp, keys) = (dir.join("game.nsp"), dir.join("prod.keys"));
        std::fs::write(&nsp, b"not an nsp").unwrap();
        std::fs::write(&keys, "").unwrap();
        let c = |p: &std::path::Path| CString::new(p.to_string_lossy().as_bytes()).unwrap();
        let ok = unsafe {
            mmk_import_nsp(
                c(&nsp).as_ptr(),
                std::ptr::null(),
                c(&keys).as_ptr(),
                c(&dir.join("out")).as_ptr(),
                std::ptr::null(),
                None,
                std::ptr::null_mut(),
            )
        };
        assert!(!ok);
        let err = unsafe { CStr::from_ptr(crate::mmk_last_error()) }.to_string_lossy();
        assert!(err.contains("prod.keys"), "{err}");
        assert!(!unsafe {
            mmk_import_nsp(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                None,
                std::ptr::null_mut(),
            )
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_arguments_fail_cleanly() {
        assert!(!unsafe {
            mmk_import_run(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                None,
                std::ptr::null_mut(),
            )
        });
    }
}

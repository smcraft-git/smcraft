//! First-launch setup for the players without a setup screen of their own (Windows, Linux,
//! Android): the player puts their own dump, the SMM2 NSP (and the 3.0.3 update's) plus
//! `prod.keys`, into the data folder's `import/` folder; the game imports it once into the
//! asset pack (`mmk_import::run_nsp`) on a thread while it shows the progress, then starts the
//! front end. After that the dump and the keys are never read again and can be deleted.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A dump found in the drop folder.
#[derive(Clone, Debug, PartialEq)]
pub struct DropDump {
    /// The NSPs, base and update in no particular order.
    pub nsps: Vec<PathBuf>,
    pub keys: PathBuf,
}

/// The drop folder: `<data root>/import`.
pub fn drop_folder() -> Option<PathBuf> {
    mmk_host::paths::data_root().map(|d| d.join("import"))
}

/// What to tell a player who has no import yet: a log line and the two lines on screen.
pub fn missing_message(drop: &Path) -> (String, (String, String)) {
    (
        format!(
            "no SMM2 import yet: put your SMM2 NSP (and the 3.0.3 update's) and prod.keys in {}, \
             or run `mmk import --nsp`",
            drop.display()
        ),
        (
            if mmk_host::paths::outdated_import() {
                "YOUR IMPORT IS FROM AN OLDER SMCRAFT: COPY YOUR NSP AND PROD.KEYS TO".into()
            } else {
                "COPY YOUR SMM2 NSP AND PROD.KEYS TO".into()
            },
            drop.display().to_string().to_ascii_uppercase(),
        ),
    )
}

/// The NSPs and `prod.keys` in `dir`, if both are there.
pub fn find_dump(dir: &Path) -> Option<DropDump> {
    let mut nsps = Vec::new();
    let mut keys = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".nsp") {
            nsps.push(p);
        } else if name == "prod.keys" || name == "keys.txt" {
            keys = Some(p);
        }
    }
    nsps.sort();
    (!nsps.is_empty() && nsps.len() <= 2).then_some(())?;
    Some(DropDump { nsps, keys: keys? })
}

/// Where an import is.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Running {
        stage: String,
        done: usize,
        total: usize,
    },
    Done(Result<(), String>),
}

impl Status {
    /// Two lines for the screen.
    pub fn lines(&self) -> (String, String) {
        match self {
            Status::Running { stage, done, total } => {
                let pct = if *total > 0 { done * 100 / total } else { 0 };
                (
                    "IMPORTING YOUR GAME".to_string(),
                    format!("{} {pct}%", stage.to_ascii_uppercase()),
                )
            }
            Status::Done(Ok(())) => ("IMPORT DONE".to_string(), String::new()),
            Status::Done(Err(e)) => ("IMPORT FAILED".to_string(), e.to_ascii_uppercase()),
        }
    }
}

/// An import running on its own thread.
pub struct Import {
    status: Arc<Mutex<Status>>,
}

impl Import {
    /// Import `dump` into `out_root` (the assets folder).
    pub fn start(dump: DropDump, out_root: PathBuf) -> Self {
        let status = Arc::new(Mutex::new(Status::Running {
            stage: "open".into(),
            done: 0,
            total: 1,
        }));
        let s = status.clone();
        std::thread::spawn(move || {
            let mut progress = |stage: &str, done: usize, total: usize| {
                if let Ok(mut st) = s.lock() {
                    *st = Status::Running {
                        stage: stage.to_string(),
                        done,
                        total,
                    };
                }
            };
            let result = run(&dump, &out_root, &mut progress);
            if let Ok(mut st) = s.lock() {
                *st = Status::Done(result);
            }
        });
        Self { status }
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(Status::Done(Err("import thread failed".into())))
    }
}

/// Which NSP is the base game and which the update isn't known from the names: try both ways.
fn run(dump: &DropDump, out_root: &Path, progress: mmk_import::Progress) -> Result<(), String> {
    let orders: Vec<(PathBuf, Option<PathBuf>)> = match dump.nsps.as_slice() {
        [one] => vec![(one.clone(), None)],
        [a, b] => vec![(a.clone(), Some(b.clone())), (b.clone(), Some(a.clone()))],
        _ => return Err("put one or two NSP files in the import folder".into()),
    };
    let mut last = String::new();
    for (base, update) in orders {
        let cfg = mmk_import::NspImportConfig {
            base,
            update,
            keys: dump.keys.clone(),
            out_root: out_root.to_path_buf(),
            language: "USen".into(),
        };
        match mmk_import::run_nsp(&cfg, &mut *progress) {
            Ok(_) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dump_needs_nsps_and_the_keys() {
        let dir = std::env::temp_dir().join(format!("mmk-setup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(find_dump(&dir), None);
        std::fs::write(dir.join("Game.NSP"), b"").unwrap();
        assert_eq!(find_dump(&dir), None, "no keys");
        std::fs::write(dir.join("prod.keys"), b"").unwrap();
        std::fs::write(dir.join("update.nsp"), b"").unwrap();
        let d = find_dump(&dir).unwrap();
        assert_eq!(d.nsps.len(), 2);
        assert_eq!(d.keys, dir.join("prod.keys"));
        std::fs::write(dir.join("third.nsp"), b"").unwrap();
        assert_eq!(find_dump(&dir), None, "too many NSPs");
        let _ = std::fs::remove_dir_all(&dir);
        // progress text
        let s = Status::Running {
            stage: "textures".into(),
            done: 1,
            total: 4,
        };
        assert_eq!(s.lines().1, "TEXTURES 25%");
    }
}

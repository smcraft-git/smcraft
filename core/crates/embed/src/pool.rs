//! Course World courses for the 100 Mario / Endless runs of an app's front end: a local course
//! store (an [`mmk_server::Store`] in the app's data) that the upstream fetcher
//! ([`mmk_server::upstream`]) fills from TGRCode's Endless Challenge pool, decrypting with the
//! asset pack's course key table. The fetcher asks through the app's own TGRCode client
//! ([`mmk_online::tgrcode::TgrCode`]), so its requests are paced with Course World's (one at a
//! time in the process) and cached with them. It starts once per process, the first time the
//! runs ask for courses; what it gathered stays on disk, so the runs have courses offline.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use mmk_formats::smm2::KeyTable;
use mmk_modes::endless::Difficulty;
use mmk_online::tgrcode::{Level, TgrCode, TgrError, TgrList};
use mmk_server::upstream::{self, CourseKeys, Options, Source, SourceError};
use mmk_server::{Meta, Store};
use serde_json::Value;

/// Courses gathered per difficulty and launch (a run is 6–16 courses).
pub const PER_DIFFICULTY: usize = 10;

/// A held course: its file (the plain course data) and what the source said about it.
#[derive(Clone, Debug)]
pub struct PoolCourse {
    pub path: PathBuf,
    pub meta: Meta,
}

/// The upstream fetcher's source over the app's TGRCode client.
struct Tgr(Arc<TgrCode>);

fn err(e: TgrError) -> SourceError {
    match e {
        TgrError::Busy(after) => SourceError::Busy(after),
        e => SourceError::Failed(e.to_string()),
    }
}

impl Source for Tgr {
    fn endless(&mut self, d: Difficulty, count: usize) -> Result<Vec<Value>, SourceError> {
        let level = match d {
            Difficulty::Easy => Level::Easy,
            Difficulty::Normal => Level::Normal,
            Difficulty::Expert => Level::Expert,
            Difficulty::SuperExpert => Level::SuperExpert,
        };
        self.0.list(&TgrList::Endless(level), count).map_err(err)
    }

    fn course(&mut self, code: &str) -> Result<Vec<u8>, SourceError> {
        self.0.data(code).map_err(err)
    }
    // (no thumbnails: the runs don't show them, and the fetcher would warm every course's)
}

pub struct Pool {
    store: Arc<Store>,
    dir: PathBuf,
}

static POOL: OnceLock<Option<Pool>> = OnceLock::new();

impl Pool {
    /// The process's pool in `dir` (opened and its fetcher started on the first call; later
    /// calls return the first result).
    pub fn get(dir: &Path, tgr: &Arc<TgrCode>, table: KeyTable) -> Option<&'static Pool> {
        POOL.get_or_init(|| Self::start(dir, tgr.clone(), table))
            .as_ref()
    }

    fn start(dir: &Path, tgr: Arc<TgrCode>, table: KeyTable) -> Option<Pool> {
        let store = match Store::open(dir) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                eprintln!("[pool] {}: {e:?}", dir.display());
                return None;
            }
        };
        let opts = Options {
            batch: PER_DIFFICULTY,
            pool: [PER_DIFFICULTY; 4],
            ..Default::default()
        };
        upstream::start(
            store.clone(),
            Box::new(Tgr(tgr)),
            CourseKeys::Table(Box::new(table)),
            opts,
        );
        Some(Pool {
            store,
            dir: dir.to_path_buf(),
        })
    }

    /// The courses held (the ones fetched so far, this launch and before).
    pub fn courses(&self) -> Vec<PoolCourse> {
        held(&self.store, &self.dir)
    }
}

fn held(store: &Store, dir: &Path) -> Vec<PoolCourse> {
    store
        .search("", None, "new", 0, 100)
        .into_iter()
        .filter(|m| m.upstream)
        .map(|meta| PoolCourse {
            path: dir.join(format!("courses/{}.bin", meta.code)),
            meta,
        })
        .filter(|c| c.path.is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_courses_are_the_upstream_ones() {
        let dir = std::env::temp_dir().join(format!("mmk-pool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap();
        let body = mmk_formats::smm2::Course::blank("M1").to_bytes().unwrap();
        store.upload(&body, "local").unwrap();
        let meta = Meta {
            code: "N24-N99-PNF".into(),
            data_id: 1,
            name: "From upstream".into(),
            description: String::new(),
            maker: "Ada".into(),
            style: String::new(),
            theme: 0,
            uploaded: 0,
            plays: 1,
            clears: 1,
            attempts: 4,
            likes: 0,
            boos: 0,
            owner: 0,
            maker_id: String::new(),
            record_ms: 0,
            record_holder: String::new(),
            record_verified: false,
            first_clear: String::new(),
            upstream: true,
            based_on: String::new(),
            based_on_maker: String::new(),
        };
        store.import(&body, meta).unwrap();
        let h = held(&store, &dir);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].meta.name, "From upstream");
        assert_eq!(std::fs::read(&h[0].path).unwrap(), body);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

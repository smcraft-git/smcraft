//! Course World courses for the 100 Mario / Endless runs in the browser (the web build's
//! `mmk_embed::pool`): with Course World on TGRCode, its Endless Challenge pool
//! ([`PER_DIFFICULTY`] courses per difficulty and visit, decrypted with the game data's course
//! key table), fetched in the background the first time the runs ask for courses and kept in
//! the player's storage (OPFS `mmk-save/pool/`, the last [`HELD`]), so the runs have them on
//! the next visit and offline; with Course World on the own course server, its random pool
//! (`/api/pool`, every difficulty; kept for the visit only). The runs pick by each course's
//! clear rate, as natively.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mmk_front::CourseEntry;
use mmk_online::tgrcode::{self, Level, TgrList};

use crate::app::log;
use crate::web_front::{LibCourse, course_body, remove_save, write_save};
use crate::web_server::Server;
use crate::web_tgr::Tgr;

/// Courses fetched per difficulty and visit (a run is 6–16 courses), as `mmk_embed::pool`.
pub const PER_DIFFICULTY: usize = 10;
/// Courses kept in the player's storage at most (the oldest go first).
pub const HELD: usize = 100;
/// The own server's pool asked for per visit.
const SERVER_POOL: usize = 40;
/// Where the TGRCode courses are kept in `mmk-save/`: `<code>.course` (the course data,
/// zlib-compressed) and `<code>.json` (TGRCode's record of it).
const DIR: &str = "pool";

/// One source's courses (oldest first) and whether its fetch started.
#[derive(Default)]
struct Pool {
    courses: RefCell<Vec<LibCourse>>,
    started: Cell<bool>,
}

impl Pool {
    fn has(&self, code: &str) -> bool {
        let id = id_of(code);
        self.courses.borrow().iter().any(|c| c.entry.id == id)
    }

    /// Add `c`; past [`HELD`] courses the first held are dropped (and their files, `kept`).
    fn add(&self, c: LibCourse, kept: bool) {
        let mut v = self.courses.borrow_mut();
        v.retain(|x| x.entry.id != c.entry.id);
        v.push(c);
        while v.len() > HELD {
            let old = v.remove(0);
            if kept && let Some(code) = old.entry.id.strip_prefix("pool:") {
                remove_save(format!("{DIR}/{code}.course"));
                remove_save(format!("{DIR}/{code}.json"));
            }
        }
    }

    fn entries(&self) -> Vec<CourseEntry> {
        self.courses
            .borrow()
            .iter()
            .map(|c| c.entry.clone())
            .collect()
    }
}

thread_local! {
    /// TGRCode's pool, and the own server's (by base URL).
    static TGR: Rc<Pool> = Rc::default();
    static SERVER: RefCell<Option<(String, Rc<Pool>)>> = const { RefCell::new(None) };
}

/// A pool course's library id.
fn id_of(code: &str) -> String {
    format!("pool:{code}")
}

/// A listing's course as a pool course (its own name and statistics).
fn pool_course(l: &mmk_online::Listing, body: &[u8]) -> Result<LibCourse, String> {
    let mut c = LibCourse::new(id_of(&l.code), &l.code, false, body, None)?;
    if !l.name.is_empty() {
        c.entry.name = l.name.clone();
    }
    c.entry.meta = crate::front_course::meta_of_listing(l);
    Ok(c)
}

/// The TGRCode courses kept by earlier visits, back into the pool (at start, in the background).
pub async fn restore() {
    let pool = TGR.with(Rc::clone);
    let mut names = crate::web::list_save(DIR).await;
    names.retain(|n| n.ends_with(".json"));
    for n in names {
        let code = n.trim_end_matches(".json");
        let (Some(json), Some(body)) = (
            crate::web::read_save(&format!("{DIR}/{n}")).await,
            crate::web::read_save(&format!("{DIR}/{code}.course")).await,
        ) else {
            continue;
        };
        let listing = serde_json::from_slice(&json)
            .ok()
            .and_then(|v| tgrcode::listing(&v));
        if let Some(l) = listing
            && let Ok(c) = pool_course(&l, &body)
        {
            pool.add(c, true);
        }
    }
}

/// The TGRCode courses held for the runs; the first call starts fetching more.
pub fn tgrcode(tgr: &Tgr, keys: Option<Vec<u8>>) -> Vec<CourseEntry> {
    let pool = TGR.with(Rc::clone);
    if !pool.started.replace(true) {
        let (tgr, p) = (tgr.clone(), pool.clone());
        wasm_bindgen_futures::spawn_local(fill_tgrcode(p, tgr, keys));
    }
    let out = pool.entries();
    log(&format!(
        "100 Mario / Endless: {} TGRCode courses held for the runs",
        out.len()
    ));
    out
}

async fn fill_tgrcode(pool: Rc<Pool>, tgr: Tgr, keys: Option<Vec<u8>>) {
    let mut added = 0;
    for level in [
        Level::Easy,
        Level::Normal,
        Level::Expert,
        Level::SuperExpert,
    ] {
        let list = tgr
            .run_background(|c| c.list(&TgrList::Endless(level), PER_DIFFICULTY))
            .await;
        let records = match list {
            Ok(r) => r,
            Err(e) => {
                log(&format!(
                    "[pool] {level:?}: {e} (the runs play the courses held)"
                ));
                continue;
            }
        };
        for rec in records {
            let Some(l) = tgrcode::listing(&rec) else {
                continue;
            };
            if pool.has(&l.code) {
                continue;
            }
            let code = l.code.clone();
            let got = tgr
                .run_background(|c| c.data(&code))
                .await
                .map_err(|e| e.to_string())
                .and_then(|raw| course_body(&raw, keys.as_deref()))
                .and_then(|body| pool_course(&l, &body));
            match got {
                Ok(c) => {
                    write_save(format!("{DIR}/{code}.course"), c.packed().to_vec());
                    write_save(format!("{DIR}/{code}.json"), rec.to_string().into_bytes());
                    pool.add(c, true);
                    added += 1;
                }
                Err(e) => log(&format!("[pool] {code}: {e}")),
            }
        }
    }
    log(&format!(
        "[pool] {added} new TGRCode courses ({} held for the runs)",
        pool.courses.borrow().len()
    ));
}

/// The own server's courses held for the runs; the first call for a server starts fetching.
pub fn server(server: &Server) -> Vec<CourseEntry> {
    let pool = SERVER.with(|s| {
        let mut s = s.borrow_mut();
        match &*s {
            Some((base, p)) if *base == server.base => p.clone(),
            _ => {
                let p = Rc::<Pool>::default();
                *s = Some((server.base.clone(), p.clone()));
                p
            }
        }
    });
    if !pool.started.replace(true) {
        let (server, p) = (server.clone(), pool.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let list = server.run(None, |c| c.pool((0, 100), SERVER_POOL)).await;
            let list = match list {
                Ok(l) => l,
                Err(e) => {
                    log(&format!("[pool] {}: {e}", server.base));
                    return;
                }
            };
            for l in list {
                let got = server
                    .run(None, |c| c.download(&l.code))
                    .await
                    .and_then(|body| pool_course(&l, &body));
                match got {
                    Ok(c) => p.add(c, false),
                    Err(e) => log(&format!("[pool] {}: {e}", l.code)),
                }
            }
            log(&format!(
                "[pool] {} courses of {} held for the runs",
                p.courses.borrow().len(),
                server.base
            ));
        });
    }
    pool.entries()
}

/// A pool course's decoded data, by library id (None: not a pool course, or not held).
pub fn course(id: &str) -> Option<Result<Vec<u8>, String>> {
    if !id.starts_with("pool:") {
        return None;
    }
    let find = |p: &Pool| {
        p.courses
            .borrow()
            .iter()
            .find(|c| c.entry.id == id)
            .map(LibCourse::raw)
    };
    TGR.with(|p| find(p))
        .or_else(|| SERVER.with(|s| s.borrow().as_ref().and_then(|(_, p)| find(p))))
}

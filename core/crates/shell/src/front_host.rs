//! The portable shell as the front end's [`mmk_front::Host`]: the engine (the shell's [`Game`]),
//! the course library (a folder of course files plus the asset cache's built-in courses, with
//! the dataset's `index.json` statistics), Course World's source (`mmk_online::world`: TGRCode,
//! the course server or the library; calls on threads), the runs' pool of TGRCode courses
//! (`mmk_embed::pool`), and small files for save data and settings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use mmk_front::host::{CourseMeta, LoadedCourse};
use mmk_front::netplay::{LinkState, MatchRequest, MatchState, NetAccount, Network};
use mmk_front::{CourseEntry, CourseStatus, Host, Source, WorldCourse};
use mmk_host::audio::AudioOut;
use mmk_online::world::Backend;
use serde_json::{Value, json};

pub use crate::front_course::fnv1a64;
use crate::front_course::{self, natural};
use crate::game::Game;
use crate::source::{self, AssetSource, DirSource};

/// What the host keeps between ticks.
pub struct HostData {
    /// The asset cache (None: placeholder art).
    pub source: Option<DirSource>,
    /// Course folders.
    pub library_dirs: Vec<PathBuf>,
    /// The SMM2 UI import (its `courses/` stand in for the title courses).
    pub ui_dir: Option<PathBuf>,
    /// The SMM1 UI import (its `ui/sound/` plays the SMM1 screens' sounds).
    pub smm1_dir: Option<PathBuf>,
    /// Course server base URL.
    pub server: Option<String>,
    /// Where Course World's courses come from (`MmkFrontOptions`-like setting, see main).
    pub backend: Backend,
    /// TGRCode's cache folder (for a backend made again from the Mods page).
    pub tgr_cache: Option<PathBuf>,
    /// Where save data and settings go (None: nowhere, captures).
    pub data_dir: Option<PathBuf>,
    /// The player's buttons this tick (engine bits).
    pub input: u32,
    /// Players' controls connected: the keyboard (or touch) and each pad (set by the app).
    pub controllers: u32,
    /// Play Together's players' buttons this tick (engine bits, player 1 first; set by the app
    /// while more than one plays).
    pub players: Vec<u32>,
    /// Sound output for courses.
    pub audio: Option<Arc<AudioOut>>,
    /// The Course Maker's music and edit sounds (made from `audio` when first needed).
    maker_sounds: Option<mmk_host::audio::Director>,
    title: Option<Vec<PathBuf>>,
    library: Option<Vec<CourseEntry>>,
    scanning: Option<Receiver<Vec<CourseEntry>>>,
    rx: Option<Receiver<Vec<WorldCourse>>>,
    rng: u64,
    memory: HashMap<String, String>,
    /// Server calls (`Host::online`) answer here, from their threads.
    online: Option<(Sender<Reply>, Receiver<Reply>)>,
    /// The record run downloaded by a `ghost` call: raced from the next start of that course.
    ghost: Option<(String, mmk_modes::replay::Replay)>,
    /// Thumbnails from their threads: (code, entire, JPEG; empty: none).
    thumbs: Option<(Sender<Thumbnail>, Receiver<Thumbnail>)>,
    /// The runs' pool entries made so far, by file.
    pool_entries: HashMap<PathBuf, Option<CourseEntry>>,
    /// Coursebot courses saved without a data folder (captures): ("memory:<n>", raw data).
    made: Vec<(String, Vec<u8>)>,
    /// Network Play (None: not set up; `mmk-play --nextendo-local`).
    pub network: Option<crate::network::NetworkHost>,
}

/// A thumbnail from its thread: (code, entire, JPEG; empty: none).
type Thumbnail = (String, bool, Vec<u8>);

/// A server call's id and what its thread sent back.
type Reply = (u32, Answer);

/// What a server call's thread sends back.
enum Answer {
    Json(Result<Value, String>),
    /// A `ghost` call: the course code and the replay's bytes.
    Ghost(String, Result<Vec<u8>, String>),
    /// A `download` call: the file written into the library.
    Download(Result<PathBuf, String>),
}

impl HostData {
    /// The Course Maker's sound director (None: no sound output).
    fn maker_director(&mut self) -> Option<&mut mmk_host::audio::Director> {
        if self.maker_sounds.is_none() {
            self.maker_sounds = self.audio.as_ref().map(|a| a.director());
        }
        self.maker_sounds.as_mut()
    }

    /// A persistent value (the data folder's file for it, or memory for captures).
    pub fn load_value(&self, key: &str) -> Option<String> {
        match &self.data_dir {
            Some(d) => std::fs::read_to_string(d.join(file_name(key))).ok(),
            None => self.memory.get(key).cloned(),
        }
    }

    pub fn store_value(&mut self, key: &str, value: &str) {
        match &self.data_dir {
            Some(d) => {
                let _ = std::fs::create_dir_all(d);
                let _ = std::fs::write(d.join(file_name(key)), value);
            }
            None => {
                self.memory.insert(key.to_string(), value.to_string());
            }
        }
    }

    pub fn new(source: Option<DirSource>) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64)
            | 1;
        Self {
            source,
            library_dirs: Vec::new(),
            ui_dir: None,
            smm1_dir: None,
            server: None,
            backend: Backend::Library,
            tgr_cache: None,
            data_dir: None,
            input: 0,
            controllers: 1,
            players: Vec::new(),
            audio: None,
            maker_sounds: None,
            title: None,
            library: None,
            scanning: None,
            rx: None,
            rng: seed,
            memory: HashMap::new(),
            online: None,
            ghost: None,
            thumbs: None,
            pool_entries: HashMap::new(),
            made: Vec::new(),
            network: None,
        }
    }

    /// Where the Coursebot's courses are saved (`<data>/Coursebot`), if there is a data folder.
    pub fn coursebot_dir(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("Coursebot"))
    }

    /// The id of Coursebot slot `slot`: `<data>/Coursebot/course_NNN.bin` (NNN = slot + 1),
    /// or `memory:NNN` without a data folder.
    pub fn slot_id(&self, slot: usize) -> String {
        let name = format!("course_{:03}.bin", slot + 1);
        match self.coursebot_dir() {
            Some(d) => d.join(name).to_string_lossy().into_owned(),
            None => format!("memory:{name}"),
        }
    }

    /// The slot an id is (None: not a Coursebot slot).
    pub fn slot_of(&self, id: &str) -> Option<usize> {
        (0..mmk_front::host::COURSEBOT_SLOTS).find(|&s| self.slot_id(s) == id)
    }

    /// A library course's file bytes (a Coursebot course kept in memory, or a file).
    fn course_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        if let Some((_, b)) = self.made.iter().find(|(i, _)| i == id) {
            return Ok(b.clone());
        }
        std::fs::read(id).map_err(|e| format!("{id}: {e}"))
    }

    /// The library's folders and the Coursebot's.
    fn scan_dirs(&self) -> Vec<PathBuf> {
        let mut d = self.library_dirs.clone();
        if let Some(c) = self.coursebot_dir()
            && !d.contains(&c)
        {
            d.push(c);
        }
        d
    }

    fn online_tx(&mut self) -> Sender<Reply> {
        self.online
            .get_or_insert_with(std::sync::mpsc::channel)
            .0
            .clone()
    }

    fn src(&self) -> Option<&dyn AssetSource> {
        self.source.as_ref().map(|s| s as &dyn AssetSource)
    }

    fn decode(&self, bytes: &[u8]) -> Result<(mmk_formats::smm2::Course, String), String> {
        decode(self.src(), bytes)
    }

    /// The built-in courses' folders: the asset cache's (Story Mode, lessons, title courses) and
    /// the SMM1 import's (its sample courses, `mmk_import::smm1_samples`).
    fn built_in_dirs(&self) -> Vec<PathBuf> {
        let root = self.source.as_ref().and_then(|s| s.dir());
        [root, self.smm1_dir.as_deref()]
            .into_iter()
            .flatten()
            .map(|d| d.join("courses"))
            .collect()
    }

    /// Scan the library on a thread (`library` waits for it).
    pub fn start_scan(&mut self) {
        let dirs = self.scan_dirs();
        let built_in = self.built_in_dirs();
        let root = self
            .source
            .as_ref()
            .and_then(|s| s.dir())
            .map(Path::to_path_buf);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(scan(&dirs, &built_in, root.as_deref()));
        });
        self.scanning = Some(rx);
    }

    /// The title demo courses the engine can play (the asset cache's built-in title courses,
    /// else the UI import's copies), in their numbering order.
    fn title_files(&mut self) -> &[PathBuf] {
        if self.title.is_none() {
            let mut list: Vec<PathBuf> = Vec::new();
            let dirs: Vec<PathBuf> = [
                self.source
                    .as_ref()
                    .and_then(|s| s.dir())
                    .map(Path::to_path_buf),
                self.ui_dir.clone(),
            ]
            .into_iter()
            .flatten()
            .map(|d| d.join("courses"))
            .collect();
            for dir in dirs {
                if !list.is_empty() {
                    break;
                }
                let Ok(rd) = std::fs::read_dir(&dir) else {
                    continue;
                };
                list = rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("title_course_data_"))
                    })
                    .collect();
                list.sort_by(|a, b| natural(&a.to_string_lossy(), &b.to_string_lossy()));
            }
            let playable: Vec<PathBuf> = list
                .iter()
                .filter(|p| {
                    std::fs::read(p)
                        .ok()
                        .and_then(|b| self.decode(&b).ok())
                        .is_some_and(|(c, _)| front_course::playable(&c))
                })
                .cloned()
                .collect();
            self.title = Some(if playable.is_empty() { list } else { playable });
        }
        self.title.as_deref().unwrap_or(&[])
    }

    fn load_file(&self, game: &mut Game, path: &Path) -> Result<LoadedCourse, String> {
        let bytes = self.course_bytes(&path.to_string_lossy())?;
        self.load_bytes(game, &bytes)
    }

    fn load_bytes(&self, game: &mut Game, bytes: &[u8]) -> Result<LoadedCourse, String> {
        let (c, key) = self.decode(bytes)?;
        let loaded = LoadedCourse {
            name: c.header.name.to_string_lossy(),
            key: Some(key),
            theme: c.main.header.theme,
        };
        game.set_course(Some(c));
        game.session.set_audio(self.audio.clone());
        Ok(loaded)
    }
}

impl HostData {
    /// The course key table's bytes (TGRCode's downloads decrypt with it).
    fn key_table(&self) -> Result<Vec<u8>, String> {
        source::key_table(self.src())
    }

    fn world_table(&self) -> Result<Vec<u8>, String> {
        source::world_table(self.src())
    }

    /// Where downloads go: the first library folder's `Downloaded/` (else the data folder's).
    fn download_dir(&self) -> Option<PathBuf> {
        self.library_dirs
            .first()
            .or(self.data_dir.as_ref())
            .map(|d| d.join("Downloaded"))
    }

    /// The library entry of a course file (None: not a course the engine reads).
    fn entry_of(&self, path: &Path, meta: CourseMeta) -> Option<CourseEntry> {
        let bytes = std::fs::read(path).ok()?;
        let (c, key) = self.decode(&bytes).ok()?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut e = front_course::entry(
            path.to_string_lossy().into_owned(),
            &stem,
            false,
            &c,
            key,
            None,
        );
        e.meta = meta;
        Some(e)
    }
}

/// A Course World listing's statistics as library metadata.
fn meta_of_pool(m: &mmk_server::Meta) -> CourseMeta {
    let n = |v: u64| Some(v as i64);
    CourseMeta {
        clear_rate: (m.attempts > 0).then(|| m.clears as f64 * 100.0 / m.attempts as f64),
        bucket: None,
        maker: Some(m.maker.clone()).filter(|s| !s.is_empty()),
        code: Some(m.code.clone()),
        world_record_ms: (m.record_ms > 0).then_some(m.record_ms as i64),
        plays: n(m.plays),
        likes: n(m.likes),
        clears: n(m.clears),
        attempts: n(m.attempts),
        uploaded: (m.uploaded > 0).then_some(m.uploaded as i64),
        tags: Vec::new(),
    }
}

/// The library folders' courses and the built-in folders' (`built_in`), sorted by name; the
/// asset cache at `asset_root` decrypts `.bcd` files.
fn scan(
    library_dirs: &[PathBuf],
    built_in: &[PathBuf],
    asset_root: Option<&Path>,
) -> Vec<CourseEntry> {
    let source = asset_root.map(DirSource::new);
    let src = source.as_ref().map(|s| s as &dyn AssetSource);
    let mut out = Vec::new();
    let mut sources: Vec<(PathBuf, bool)> = Vec::new();
    let mut meta: HashMap<PathBuf, CourseMeta> = HashMap::new();
    for dir in library_dirs {
        for f in files(dir) {
            if f.file_name().is_some_and(|n| n == "index.json") {
                if let Ok(text) = std::fs::read_to_string(&f) {
                    let base = f.parent().unwrap_or(Path::new(""));
                    for (file, m) in CourseMeta::parse_index(&text, |id| {
                        mmk_formats::course_id::smm2_data_id_to_code(id, false)
                    }) {
                        let p = base.join(file);
                        meta.insert(p.canonicalize().unwrap_or(p), m);
                    }
                }
            } else if candidate(&f) {
                sources.push((f, false));
            }
        }
    }
    for dir in built_in {
        sources.extend(
            files(dir)
                .into_iter()
                .filter(|f| candidate(f))
                .map(|f| (f, true)),
        );
    }
    for (path, built_in) in sources {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok((c, key)) = decode(src, &bytes) else {
            continue;
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let canon = path.canonicalize().unwrap_or(path.clone());
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let mut e = front_course::entry(
            path.to_string_lossy().into_owned(),
            &stem,
            built_in,
            &c,
            key,
            modified,
        );
        e.meta = meta.remove(&canon).unwrap_or_default();
        out.push(e);
    }
    out.sort_by(|a, b| natural(&a.name, &b.name));
    out
}

fn decode(
    src: Option<&dyn AssetSource>,
    bytes: &[u8],
) -> Result<(mmk_formats::smm2::Course, String), String> {
    front_course::decode(bytes, || source::key_table(src)).map(|(c, _, key)| (c, key))
}

/// Likely course files: `.course.zlib`, `.zlib`, `.bcd`, raw data (`.bin`, `.dat`, `.course`,
/// no extension), at most 1 MiB.
fn candidate(p: &Path) -> bool {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    if name.starts_with('.') {
        return false;
    }
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    ["zlib", "bcd", "bin", "dat", "course", ""].contains(&ext.as_str())
        && std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= 1 << 20)
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// The host for one tick: the game and the data.
pub struct ShellHost<'a> {
    pub game: &'a mut Game,
    pub data: &'a mut HostData,
}

impl Host for ShellHost<'_> {
    fn library(&mut self) -> Vec<CourseEntry> {
        if self.data.library.is_none() {
            let found = match self.data.scanning.take() {
                Some(rx) => rx.recv().unwrap_or_default(),
                None => {
                    let root = self.data.source.as_ref().and_then(|s| s.dir());
                    scan(&self.data.scan_dirs(), &self.data.built_in_dirs(), root)
                }
            };
            let mut found = found;
            for (id, bytes) in &self.data.made {
                if let Ok((c, key)) = self.data.decode(bytes) {
                    found.push(front_course::entry(id.clone(), id, false, &c, key, None));
                }
            }
            self.data.library = Some(found);
        }
        self.data.library.clone().unwrap_or_default()
    }

    fn request_server_courses(&mut self) -> bool {
        self.request_world_list("popular")
    }

    fn course_world(&self) -> &str {
        self.data.backend.name()
    }

    fn world_lists_by_tab(&self) -> bool {
        self.data.backend.lists_by_tab()
    }

    fn request_world_list(&mut self, tab: &str) -> bool {
        if !self.data.backend.online() {
            return false;
        }
        let backend = self.data.backend.clone();
        let tab = tab.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let list = match backend.list(&tab) {
                Ok(l) => l.iter().filter_map(WorldCourse::server).collect(),
                Err(e) => {
                    eprintln!("[course world] {tab}: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send(list);
        });
        self.data.rx = Some(rx);
        true
    }

    fn request_thumbnail(&mut self, code: &str, entire: bool) -> bool {
        if !self.data.backend.online() {
            return false;
        }
        let tx = self
            .data
            .thumbs
            .get_or_insert_with(std::sync::mpsc::channel)
            .0
            .clone();
        let backend = self.data.backend.clone();
        let code = code.to_string();
        std::thread::spawn(move || {
            let jpeg = backend.thumbnail(&code, entire).unwrap_or_default();
            let _ = tx.send((code, entire, jpeg));
        });
        true
    }

    fn prefetch_course(&mut self, source: &Source) {
        // TGRCode's client keeps what it fetched (memory and disk): the load reads it from there
        if let (Source::Server(code), Backend::TgrCode(t)) = (source, &self.data.backend) {
            let (t, code) = (t.clone(), code.clone());
            std::thread::spawn(move || {
                let _ = t.data(&code);
            });
        }
    }

    fn poll_thumbnail(&mut self) -> Option<(String, bool, Vec<u8>)> {
        self.data.thumbs.as_ref()?.1.try_recv().ok()
    }

    fn challenge_courses(&mut self) -> Vec<CourseEntry> {
        let (Backend::TgrCode(tgr), Some(dir)) = (&self.data.backend, &self.data.data_dir) else {
            return Vec::new();
        };
        let Some(table) = self
            .data
            .key_table()
            .ok()
            .and_then(|b| mmk_formats::smm2::crypto::table_from_bytes(&b).ok())
        else {
            return Vec::new();
        };
        let Some(pool) = mmk_embed::pool::Pool::get(&dir.join("TGRCode/pool"), tgr, table) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for c in pool.courses() {
            if !self.data.pool_entries.contains_key(&c.path) {
                let e = self
                    .data
                    .entry_of(&c.path, meta_of_pool(&c.meta))
                    .map(|mut e| {
                        e.name = c.meta.name.clone();
                        e
                    });
                self.data.pool_entries.insert(c.path.clone(), e);
            }
            if let Some(Some(e)) = self.data.pool_entries.get(&c.path) {
                out.push(e.clone());
            }
        }
        crate::app::log(&format!(
            "100 Mario / Endless: {} TGRCode courses held for the runs",
            out.len()
        ));
        out
    }

    fn poll_server_courses(&mut self) -> Option<Vec<WorldCourse>> {
        let r = self.data.rx.as_ref()?.try_recv().ok()?;
        self.data.rx = None;
        Some(r)
    }

    fn load_course(&mut self, source: &Source) -> Result<LoadedCourse, String> {
        match source {
            Source::Library(path) => self.data.load_file(self.game, Path::new(path)),
            Source::Server(code) if !matches!(self.data.backend, Backend::Server(_)) => {
                // TGRCode: its file (cached by the client), decrypted with the pack's key table
                let key_table = self.data.key_table();
                let body = self.data.backend.download(code, || key_table)?;
                self.data.load_bytes(self.game, &body)
            }
            Source::Server(code) => {
                let server = self
                    .data
                    .backend
                    .server()
                    .ok_or("no course server")?
                    .to_string();
                let dir = self
                    .data
                    .data_dir
                    .clone()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("Course World");
                let _ = std::fs::create_dir_all(&dir);
                let file = dir.join(format!("{code}.bin"));
                if !file.is_file() {
                    let bytes = mmk_online::Client::new(server)
                        .download(code)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
                }
                let loaded = self.data.load_file(self.game, &file)?;
                // a server course's clears go back as replays the server verifies
                self.game.session.record_replays();
                if self.data.ghost.as_ref().is_some_and(|g| &g.0 == code)
                    && let Some((_, r)) = self.data.ghost.take()
                {
                    self.game.session.set_ghost(r);
                }
                Ok(loaded)
            }
        }
    }

    fn title_courses(&mut self) -> usize {
        self.data.title_files().len()
    }

    fn load_title_course(&mut self, index: usize) -> bool {
        let Some(p) = self.data.title_files().get(index).cloned() else {
            return false;
        };
        self.data.load_file(self.game, &p).is_ok()
    }

    fn unload_course(&mut self) {
        if let Some(a) = &self.data.audio {
            a.send(mmk_audio::Command::StopAll);
        }
        self.game.session.set_audio(None);
        self.game.set_course(None);
        // back to one player: a co-op start sets its players again before loading
        self.game.session.set_players(1);
    }

    fn step_course(&mut self, scripted: Option<u32>) {
        let b = scripted.unwrap_or(self.data.input) & !mmk_sim::input::START;
        self.game.session.tick(mmk_sim::Input { buttons: b });
    }

    fn course_status(&self) -> CourseStatus {
        front_course::status(self.game)
    }

    fn restart_course(&mut self) {
        self.game.session.restart();
    }

    fn set_character(&mut self, character: u32) {
        if self.game.session.players() == 1 {
            self.game.session.world.main_char = (character as usize).min(3);
        }
    }

    fn set_players(&mut self, players: u32) {
        self.game.session.set_players(players as usize);
    }

    fn controllers(&mut self) -> u32 {
        self.data.controllers
    }

    fn player_buttons(&mut self, players: usize) -> Vec<u32> {
        let p = &self.data.players;
        if p.len() >= players {
            p[..players].to_vec()
        } else {
            Vec::new()
        }
    }

    fn set_run_counters(&mut self, score: u32, coins: u32, lives: u32) {
        self.game
            .session
            .world
            .set_run_counters(score, coins, lives);
    }

    fn local_offset(&self, secs: i64) -> i64 {
        local_offset(secs)
    }

    fn set_paused(&mut self, paused: bool) {
        if let Some(a) = &self.data.audio {
            a.send(mmk_audio::Command::SetVolumes {
                master: 1.0,
                music: if paused { 0.3 } else { 1.0 },
                sfx: 1.0,
            });
        }
    }

    fn report(&mut self, code: &str, what: &str) {
        if self.data.backend.server().is_none() {
            return;
        }
        let backend = self.data.backend.clone();
        let (code, what) = (code.to_string(), what.to_string());
        std::thread::spawn(move || backend.report(&code, &what));
    }

    fn online(&mut self, id: u32, action: &str, args: &Value, token: Option<&str>) -> bool {
        // the maker's uploads go to our own course server whichever Course World is shown
        // (TGRCode is read-only)
        if action == "upload" {
            let Some(server) = self.data.server.clone().filter(|s| !s.is_empty()) else {
                return false;
            };
            let tx = self.data.online_tx();
            let (token, args) = (
                token.filter(|t| !t.is_empty()).map(str::to_string),
                args.clone(),
            );
            std::thread::spawn(move || {
                let a = mmk_online::call(&server, token.as_deref(), "upload", &args);
                let _ = tx.send((id, Answer::Json(a)));
            });
            return true;
        }
        if !self.data.backend.online() {
            return false;
        }
        let backend = self.data.backend.clone();
        let tx = self.data.online_tx();
        let token = token.filter(|t| !t.is_empty()).map(str::to_string);
        let code = args["code"].as_str().unwrap_or_default().to_string();
        if action == "download" {
            let key_table = self.data.key_table();
            let dir = self.data.download_dir();
            std::thread::spawn(move || {
                let r = (|| {
                    let dir = dir.ok_or("no library folder")?;
                    let body = backend.download(&code, || key_table)?;
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let file = dir.join(format!("{code}.course"));
                    std::fs::write(&file, body).map_err(|e| e.to_string())?;
                    Ok(file)
                })();
                let _ = tx.send((id, Answer::Download(r)));
            });
            return true;
        }
        if action == "super_world" {
            let call = id;
            // its map decrypted with the pack's world-map table (TGRCode only)
            let table = self.data.world_table();
            let id = args["id"].as_str().unwrap_or_default().to_string();
            std::thread::spawn(move || {
                let a = backend.super_world(&id, || table);
                let _ = tx.send((call, Answer::Json(a)));
            });
            return true;
        }
        let Some(server) = backend.server().map(str::to_string) else {
            // TGRCode: the read-only calls
            let (action, args) = (action.to_string(), args.clone());
            std::thread::spawn(move || {
                let a = backend.call(token.as_deref(), &action, &args);
                let _ = tx.send((id, Answer::Json(a)));
            });
            return true;
        };
        match action {
            "submit_replay" => {
                // the run just cleared (kept by the session since the course started)
                let replay = self.game.session.take_cleared_replay();
                let affects = self.play_options_affect_play();
                match replay.filter(|_| !affects) {
                    Some(r) => {
                        std::thread::spawn(move || {
                            let c = mmk_online::Client::new(server).with_token(token);
                            let a = c
                                .submit_replay(&code, &r.encode())
                                .map(|l| mmk_online::listing_json(&l));
                            let _ = tx.send((id, Answer::Json(a)));
                        });
                    }
                    None => {
                        let _ = tx.send((id, Answer::Json(Err("no replay of a clear".into()))));
                    }
                }
            }
            "ghost" => {
                std::thread::spawn(move || {
                    let bytes = mmk_online::Client::new(server).record_replay(&code);
                    let _ = tx.send((id, Answer::Ghost(code, bytes)));
                });
            }
            _ => {
                let (action, args) = (action.to_string(), args.clone());
                std::thread::spawn(move || {
                    let a = mmk_online::call(&server, token.as_deref(), &action, &args);
                    let _ = tx.send((id, Answer::Json(a)));
                });
            }
        }
        true
    }

    fn poll_online(&mut self) -> Option<(u32, Result<Value, String>)> {
        let (id, a) = self.data.online.as_ref()?.1.try_recv().ok()?;
        Some((
            id,
            match a {
                Answer::Json(r) => r,
                Answer::Download(r) => r.map(|file| {
                    // the library has it from now on
                    if let Some(e) = self.data.entry_of(&file, CourseMeta::default())
                        && let Some(lib) = &mut self.data.library
                    {
                        lib.retain(|x| x.id != e.id);
                        lib.push(e);
                    }
                    json!({ "file": file.to_string_lossy() })
                }),
                Answer::Ghost(code, bytes) => bytes.and_then(|b| {
                    let r = mmk_modes::replay::Replay::decode(&b)
                        .ok_or_else(|| "the record's run isn't a replay".to_string())?;
                    let frames = r.inputs.len();
                    let same = r.sim_version == mmk_modes::replay::SIM_VERSION;
                    self.data.ghost = Some((code, r));
                    Ok(json!({ "frames": frames, "same_engine": same }))
                }),
            },
        ))
    }

    fn play_options_affect_play(&self) -> bool {
        self.game.session.world.options.affects_play()
    }

    fn edit_show(&mut self, course: &[u8], area: usize) -> bool {
        match mmk_formats::smm2::Course::parse(course) {
            Ok(c) => {
                self.game.show_edit(c, area);
                true
            }
            Err(_) => false,
        }
    }

    fn edit_camera(&mut self, x: f32, y: f32) {
        self.game.session.world.look_at(x, y);
    }

    fn edit_music(&mut self, track: Option<(&str, u8)>) {
        let Some(d) = self.data.maker_director() else {
            return;
        };
        match track {
            Some((style, theme)) => {
                d.edit_music(style, theme);
            }
            None => d.stop_music(),
        }
    }

    fn edit_sound(&mut self, style: &str, user: &str, key: &str, scale: Option<f32>) {
        if let Some(d) = self.data.maker_director() {
            d.edit_sound(style, user, key, scale);
        }
    }

    fn map_music(&mut self, theme: Option<u8>) {
        let Some(d) = self.data.maker_director() else {
            return;
        };
        match theme {
            Some(t) => {
                d.map_music(t);
            }
            None => d.stop_music(),
        }
    }

    fn map_sound(&mut self, user: &str, key: &str, anim: bool, point: Option<&str>) {
        if let Some(d) = self.data.maker_director() {
            d.map_sound(user, key, anim, point);
        }
    }

    fn test_play(
        &mut self,
        course: &[u8],
        area: usize,
        start: Option<[f32; 2]>,
    ) -> Result<LoadedCourse, String> {
        let c = mmk_formats::smm2::Course::parse(course).map_err(|e| e.to_string())?;
        let loaded = LoadedCourse {
            name: c.header.name.to_string_lossy(),
            key: None,
            theme: c.main.header.theme,
        };
        self.game.test_course(c, area, start);
        self.game.session.set_audio(self.data.audio.clone());
        Ok(loaded)
    }

    fn player_position(&self) -> Option<(usize, [f32; 2])> {
        let w = &self.game.session.world;
        w.player
            .alive()
            .then_some((w.area, [w.player.x, w.player.y]))
    }

    fn player_snapshot(&mut self) -> Option<(mmk_front::Icon, [f32; 4])> {
        self.game.player_snapshot()
    }

    fn save_course(&mut self, id: Option<&str>, course: &[u8]) -> Result<String, String> {
        // over an entry: its slot, or the file it is; else the first free slot
        if let Some(id) = id {
            if let Some(slot) = self.data.slot_of(id) {
                return self.save_slot(slot, course);
            }
            std::fs::write(id, course).map_err(|e| format!("{id}: {e}"))?;
            self.data.library = None;
            return Ok(id.to_string());
        }
        let slots = self.coursebot_slots();
        let free = slots
            .iter()
            .position(Option::is_none)
            .ok_or("the Coursebot is full")?;
        self.save_slot(free, course)
    }

    fn coursebot_slots(&mut self) -> Vec<Option<CourseEntry>> {
        (0..mmk_front::host::COURSEBOT_SLOTS)
            .map(|slot| {
                let id = self.data.slot_id(slot);
                let bytes = self.data.course_bytes(&id).ok()?;
                let (c, key) = self.data.decode(&bytes).ok()?;
                Some(front_course::entry(id.clone(), &id, false, &c, key, None))
            })
            .collect()
    }

    fn save_slot(&mut self, slot: usize, course: &[u8]) -> Result<String, String> {
        let id = self.data.slot_id(slot);
        match self.data.coursebot_dir() {
            Some(dir) => {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(&id, course).map_err(|e| format!("{id}: {e}"))?;
            }
            None => {
                self.data.made.retain(|(m, _)| *m != id);
                self.data.made.push((id.clone(), course.to_vec()));
            }
        }
        // the library lists it from now on
        self.data.library = None;
        self.data.scanning = None;
        Ok(id)
    }

    fn delete_slot(&mut self, slot: usize) -> bool {
        let id = self.data.slot_id(slot);
        let gone = match self.data.coursebot_dir() {
            Some(_) => std::fs::remove_file(&id).is_ok(),
            None => {
                let n = self.data.made.len();
                self.data.made.retain(|(m, _)| *m != id);
                self.data.made.len() != n
            }
        };
        if gone {
            self.data.library = None;
            self.data.scanning = None;
        }
        gone
    }

    fn course_thumbnail(&mut self, course: &[u8]) -> Option<mmk_front::Icon> {
        let c = mmk_formats::smm2::Course::parse(course).ok()?;
        let i = mmk_host::icon::course_view(&self.game.cache, &c);
        Some(mmk_front::Icon {
            width: i.width,
            height: i.height,
            rgba: i.rgba,
        })
    }

    fn course_data(&mut self, source: &Source) -> Result<Vec<u8>, String> {
        let bytes = match source {
            Source::Library(id) => self.data.course_bytes(id)?,
            // Course World's source: TGRCode (decrypted with the pack's key table) or the
            // course server, as playing it does
            Source::Server(code) => {
                let key_table = self.data.key_table();
                self.data.backend.download(code, || key_table)?
            }
        };
        front_course::decode(&bytes, || source::key_table(self.data.src())).map(|(_, d, _)| d)
    }

    fn part_icon(&mut self, style: &str, part: i16, flags: u32) -> Option<mmk_front::Icon> {
        self.game.part_icon(style, part, flags)
    }

    fn attachment_icon(&mut self, style: &str, flags: u32) -> Option<mmk_front::Icon> {
        self.game.attachment_icon(style, flags)
    }

    fn set_engine_option(&mut self, name: &str, value: i32) -> bool {
        let mut o = self.game.session.options;
        if !o.set(name, value) {
            return false;
        }
        self.game.session.set_options(o);
        true
    }

    fn costumes(&mut self) -> Vec<(u16, String)> {
        self.game.cache.costumes()
    }

    fn costume_categories(&mut self) -> Vec<(u16, u8)> {
        self.game.cache.costume_categories()
    }

    fn set_course_world(&mut self, source: &str) -> bool {
        self.data.backend = Backend::from_setting(
            Some(source),
            self.data.server.as_deref(),
            self.data.tgr_cache.clone(),
        );
        true
    }

    fn load_value(&mut self, key: &str) -> Option<String> {
        self.data.load_value(key)
    }

    fn store_value(&mut self, key: &str, value: &str) {
        self.data.store_value(key, value);
    }

    fn random(&mut self) -> u64 {
        let mut x = self.data.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.data.rng = x;
        x
    }

    fn network(&mut self) -> Option<&mut dyn Network> {
        self.data.network.as_ref()?;
        Some(self)
    }
}

/// Network Play: the shell's [`crate::network::NetworkHost`], with the engine for the match.
impl Network for ShellHost<'_> {
    fn account(&mut self) -> NetAccount {
        self.data
            .network
            .as_ref()
            .map_or(NetAccount::SignedOut, |n| n.account())
    }

    fn sign_in(&mut self) {
        if let Some(n) = &mut self.data.network {
            n.sign_in();
        }
    }

    fn sign_out(&mut self) {
        if let Some(n) = &mut self.data.network {
            n.sign_out();
        }
        self.game.session.net = None;
    }

    fn find_match(&mut self, request: &MatchRequest) {
        self.game.session.net = None;
        if let Some(n) = &mut self.data.network {
            n.find_match(request);
        }
    }

    fn cancel_match(&mut self) {
        if let Some(n) = &mut self.data.network {
            n.cancel_match();
        }
    }

    fn poll_match(&mut self) -> MatchState {
        self.data
            .network
            .as_mut()
            .map_or(MatchState::Idle, |n| n.poll_match())
    }

    fn play_match(&mut self, course: &Source) -> Result<LoadedCourse, String> {
        let loaded = self.load_course(course)?;
        let (link, players, me) = self
            .data
            .network
            .as_mut()
            .ok_or("Network Play isn't set up")?
            .start_link()?;
        // the characters go by position (player 1 Mario, 2 Luigi, …) on every peer
        self.game.session.world.main_char = 0;
        self.game.session.go_online(Box::new(link), players, me);
        let name = match course {
            Source::Library(p) => Path::new(p)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Source::Server(code) => code.clone(),
        };
        crate::app::log(&format!(
            "Network Play: playing {name} as player {} of {players}",
            me + 1
        ));
        Ok(loaded)
    }

    fn link(&mut self) -> LinkState {
        self.data
            .network
            .as_mut()
            .map_or(LinkState::Lost, |n| n.link())
    }

    fn leave_match(&mut self) {
        if let Some(n) = &mut self.data.network {
            n.leave_match();
        }
        self.game.session.net = None;
    }
}

/// Local time minus UTC at `secs` (the C library's time zone; 0 where it isn't known).
#[cfg(unix)]
pub fn local_offset(secs: i64) -> i64 {
    let t = secs as libc::time_t;
    // SAFETY: localtime_r only writes the `tm` it is given
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::localtime_r(&t, &mut tm) };
    if r.is_null() { 0 } else { tm.tm_gmtoff as i64 }
}

#[cfg(not(unix))]
pub fn local_offset(_secs: i64) -> i64 {
    0
}

fn file_name(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_courses_save_to_the_coursebot_and_play_from_the_library() {
        let mut game = Game::load(None, None).unwrap();
        let mut data = HostData::new(None);
        let mut host = ShellHost {
            game: &mut game,
            data: &mut data,
        };
        let mut e = mmk_front::maker::new_course("M1");
        e.set_name("Made in the maker");
        e.place(0, 12, 1).unwrap();
        let bytes = e.save().unwrap();
        // the maker's still view and its test play
        assert!(host.edit_show(&bytes, 0));
        assert!(host.game.edit_view);
        host.test_play(&bytes, 0, Some([200.0, 16.0])).unwrap();
        assert_eq!(host.game.session.world.player.x, 200.0);
        assert_eq!(host.player_position().map(|p| p.0), Some(0));
        // saved without a data folder: kept in memory, listed, loadable, editable again
        let id = host.save_course(None, &bytes).unwrap();
        assert_eq!(id, "memory:course_001.bin");
        let lib = host.library();
        let entry = lib.iter().find(|c| c.id == id).expect("in the library");
        assert_eq!(entry.name, "Made in the maker");
        assert!(entry.playable && !entry.built_in);
        let loaded = host.load_course(&Source::Library(id.clone())).unwrap();
        assert_eq!(loaded.name, "Made in the maker");
        assert_eq!(
            host.course_data(&Source::Library(id.clone())).unwrap(),
            bytes
        );
        // saving again over the same entry
        assert_eq!(host.save_course(Some(&id), &bytes).unwrap(), id);
        assert_eq!(host.library().iter().filter(|c| c.id == id).count(), 1);
        // a part's picture from the engine's art needs the art: none in placeholder mode
        assert!(host.part_icon("M1", 0, 0).is_none());
        // the Coursebot's slots: the course in the first, the next one free; delete
        let slots = host.coursebot_slots();
        assert_eq!(slots.len(), 120);
        assert_eq!(slots[0].as_ref().map(|e| e.id.as_str()), Some(id.as_str()));
        assert!(slots[1].is_none());
        let second = host.save_slot(5, &bytes).unwrap();
        assert_eq!(second, "memory:course_006.bin");
        assert!(host.coursebot_slots()[5].is_some());
        assert!(host.delete_slot(5));
        assert!(host.coursebot_slots()[5].is_none());
        let pic = host.course_thumbnail(&bytes).unwrap();
        assert_eq!((pic.width, pic.height), (384, 216));
    }

    #[test]
    fn smm1_sample_courses_are_built_in_library_courses() {
        let dir = std::env::temp_dir().join(format!("mmk-shell-smm1lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("smm1/courses")).unwrap();
        std::fs::create_dir_all(dir.join("library")).unwrap();
        let mut e = mmk_front::maker::new_course("M3");
        e.set_name("A sample course");
        let bytes = e.save().unwrap();
        std::fs::write(dir.join("smm1/courses/smm1_sample_01.course"), &bytes).unwrap();
        std::fs::write(dir.join("library/mine.course"), &bytes).unwrap();
        let mut game = Game::load(None, None).unwrap();
        let mut data = HostData::new(None);
        data.library_dirs = vec![dir.join("library")];
        data.smm1_dir = Some(dir.join("smm1"));
        let mut host = ShellHost {
            game: &mut game,
            data: &mut data,
        };
        let lib = host.library();
        let sample = lib
            .iter()
            .find(|c| c.id.ends_with("smm1_sample_01.course"))
            .expect("listed");
        assert_eq!(sample.name, "A sample course");
        assert!(sample.built_in && sample.playable);
        assert_eq!(
            sample.collection.as_deref(),
            Some(front_course::SMM1_SAMPLE)
        );
        let mine = lib.iter().find(|c| c.id.ends_with("mine.course")).unwrap();
        assert!(!mine.built_in && mine.collection.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_makers_upload_reaches_the_course_server() {
        let dir = std::env::temp_dir().join(format!("mmk-shell-upload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = mmk_server::Store::open(&dir).unwrap();
        let port = 20000 + (std::process::id() % 20000) as u16 + 7;
        let addr = format!("127.0.0.1:{port}");
        let a2 = addr.clone();
        std::thread::spawn(move || mmk_server::serve(store, &a2));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut game = Game::load(None, None).unwrap();
        let mut data = HostData::new(None);
        data.server = Some(format!("http://{addr}"));
        // (Course World on TGRCode: uploads still go to the own server)
        data.server = Some(format!("http://{addr}"));
        let mut host = ShellHost {
            game: &mut game,
            data: &mut data,
        };
        let mut e = mmk_front::maker::new_course("M3");
        e.set_name("Uploaded from the maker");
        e.set_description("based on ABC-DEF-GHJ by Someone");
        let bytes = e.save().unwrap();
        let args = json!({ "course": mmk_front::maker::hex_rle(&bytes), "maker": "tester" });
        assert!(host.online(1, "upload", &args, None));
        let mut answer = None;
        for _ in 0..200 {
            if let Some(a) = host.poll_online() {
                answer = Some(a);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (id, a) = answer.expect("an answer");
        assert_eq!(id, 1);
        let code = a.unwrap()["code"].as_str().unwrap().to_string();
        let down = mmk_online::Client::new(format!("http://{addr}"))
            .download(&code)
            .unwrap();
        let c = mmk_formats::smm2::Course::parse(&down).unwrap();
        assert_eq!(c.header.name.to_string_lossy(), "Uploaded from the maker");
        assert!(
            c.header
                .description
                .to_string_lossy()
                .contains("based on ABC-DEF-GHJ")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn natural_order_and_keys() {
        let mut v = vec!["c10", "c2", "C1"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["C1", "c2", "c10"]);
        assert_eq!(file_name("player.character"), "player.character.json");
        assert_eq!(file_name("a/b"), "a_b.json");
    }

    #[test]
    fn the_mods_pages_engine_options_are_the_engines() {
        use mmk_front::mods::{OPTIONS, Target};
        let engine: Vec<&str> = OPTIONS
            .iter()
            .filter_map(|o| match o.target {
                Target::Engine(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(engine, mmk_sim::Options::NAMES);
        // and every value they offer is one the engine takes
        let mut o = mmk_sim::Options::default();
        for m in OPTIONS
            .iter()
            .filter(|m| matches!(m.target, Target::Engine(_)))
        {
            let Target::Engine(n) = m.target else {
                continue;
            };
            for (v, _) in m.choices(&[(3, "Costume".into())]) {
                assert!(o.set(n, v.parse().unwrap()), "{n}={v}");
            }
        }
        // the Course World key is the file the command line reads (course_world.json)
        assert_eq!(
            file_name(mmk_front::mods::COURSE_WORLD_KEY),
            "course_world.json"
        );
    }
}

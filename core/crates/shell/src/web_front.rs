//! The browser as the front end's [`mmk_front::Host`] (the web build's counterpart of
//! `front_host`): the engine is the shell's [`Game`]; the library is the built-in courses of the
//! game data in the browser's storage (read and decoded once, in the background, after the menus
//! open — the front end reads the library synchronously, the storage can't be), plus course files
//! the page opens, the Coursebot's courses and Course World downloads; save data and settings go
//! to `localStorage`. Course World is TGRCode's ([`crate::web_tgr`], fetched by the page) or the
//! library (the Mods page's setting); there is no course server listing (the web build speaks
//! the relay's WebSocket, not the server's HTTP API). The Course Maker draws, sounds and test
//! plays as natively; the Coursebot's courses and downloads are kept in the page's storage
//! (`mmkStorage.writeSave`, OPFS `mmk-save/`), which removing the game data leaves alone.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;

use mmk_front::host::{COURSEBOT_SLOTS, LoadedCourse};
use mmk_front::{CourseEntry, CourseStatus, Host, Source, WorldCourse};
use mmk_host::audio::AudioOut;
use mmk_online::tgrcode;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::front_course;
use crate::game::Game;
use crate::web_tgr::Tgr;

#[wasm_bindgen]
extern "C" {
    /// `mmkStorage.writeSave(path, bytes)`: keep a file of the player's own (`mmk-save/`).
    #[wasm_bindgen(catch, js_namespace = mmkStorage, js_name = writeSave)]
    fn storage_write_save(path: &str, bytes: &[u8]) -> Result<js_sys::Promise, JsValue>;
    /// `mmkStorage.removeSave(path)`: delete one.
    #[wasm_bindgen(catch, js_namespace = mmkStorage, js_name = removeSave)]
    fn storage_remove_save(path: &str) -> Result<js_sys::Promise, JsValue>;
}

/// Where the Coursebot's slot `slot` is kept in `mmk-save/` (`course_NNN.bin`, NNN = slot + 1,
/// as natively), zlib-compressed.
pub fn slot_file(slot: usize) -> String {
    format!("coursebot/course_{:03}.bin", slot + 1)
}

/// Where a Course World download is kept in `mmk-save/`.
pub const DOWNLOADS: &str = "downloaded";

/// Keep `bytes` as `path` of the player's files (in the background; a failure is logged).
fn write_save(path: String, bytes: Vec<u8>) {
    wasm_bindgen_futures::spawn_local(async move {
        let r = match storage_write_save(&path, &bytes) {
            Ok(p) => wasm_bindgen_futures::JsFuture::from(p).await.map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = r {
            crate::app::log(&format!("couldn't keep {path}: {e:?}"));
        }
    });
}

fn remove_save(path: String) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(p) = storage_remove_save(&path) {
            let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        }
    });
}

/// A library course: its entry and its decoded course data, zlib-compressed (a decoded course is
/// mostly zeros; the library holds a few hundred).
pub struct LibCourse {
    pub entry: CourseEntry,
    data: Vec<u8>,
}

impl LibCourse {
    /// A course file's bytes (raw, zlib or `.bcd` with `keys`) as library course `id`.
    pub fn new(
        id: String,
        stem: &str,
        built_in: bool,
        bytes: &[u8],
        keys: Option<&[u8]>,
    ) -> Result<Self, String> {
        let (c, data, key) = front_course::decode(bytes, || {
            keys.map(<[u8]>::to_vec)
                .ok_or_else(|| mmk_host::course::no_key_table("no game data imported"))
        })?;
        let entry = front_course::entry(id, stem, built_in, &c, key, None);
        Ok(Self {
            entry,
            data: pack(&data),
        })
    }

    /// The decoded course data.
    fn raw(&self) -> Result<Vec<u8>, String> {
        mmk_host::course::decode_data(&self.data, || Err("packed".into())).map(|(d, _)| d)
    }

    /// The decoded course data, zlib-compressed (as kept in the page's storage).
    pub fn packed(&self) -> &[u8] {
        &self.data
    }
}

fn pack(data: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    match z.write_all(data).and_then(|_| z.finish()) {
        Ok(out) => out,
        Err(_) => data.to_vec(),
    }
}

/// The library, shared with the scan that fills it.
pub type Library = Rc<RefCell<Vec<LibCourse>>>;

/// Put `c` into `library` (over a course with its id).
fn keep(library: &Library, c: LibCourse) {
    let mut lib = library.borrow_mut();
    lib.retain(|x| x.entry.id != c.entry.id);
    lib.push(c);
}

/// A thumbnail that arrived: (code, entire, JPEG; empty: none).
type Thumbnail = (String, bool, Vec<u8>);

/// A [`Host::online`] call's id and answer.
type Reply = (u32, Result<Value, String>);

/// What the host keeps between ticks.
pub struct HostData {
    /// The player's buttons this tick (engine bits).
    pub input: u32,
    /// Sound output for courses (after the page's first gesture).
    pub audio: Option<Arc<AudioOut>>,
    /// The Course Maker's music and edit sounds (made from `audio` when first needed).
    maker_sounds: Option<mmk_host::audio::Director>,
    /// The title demo courses the engine can play, in their numbering order.
    pub title: Vec<LibCourse>,
    /// Built-in courses (filled in the background), opened files, the Coursebot's courses and
    /// Course World downloads.
    pub library: Library,
    /// The Coursebot's slots: the raw course data in each.
    pub slots: Vec<Option<Vec<u8>>>,
    /// The game data's course and world-map key tables (TGRCode's files decrypt with them).
    pub keys: Option<Vec<u8>>,
    pub world_keys: Option<Vec<u8>>,
    /// Course World on TGRCode (None: the library).
    tgr: Option<Tgr>,
    /// A Course World list that arrived, and the number of the request it answers (older
    /// answers are dropped).
    list: Rc<RefCell<Option<Vec<WorldCourse>>>>,
    list_request: Rc<Cell<u32>>,
    /// Thumbnails that arrived.
    thumbs: Rc<RefCell<VecDeque<Thumbnail>>>,
    /// Answers of [`Host::online`] calls.
    replies: Rc<RefCell<VecDeque<Reply>>>,
    /// The style and theme of the course just loaded, whose music the page must now read
    /// (the mixer reads it synchronously; see `web::pump_ui`).
    pub music_wanted: Option<(String, u8)>,
    /// The same for the Course Maker's music and edit sounds, and the track it plays.
    pub edit_wanted: Option<(String, u8)>,
    edit_track: Option<(String, u8)>,
    rng: u64,
}

impl HostData {
    pub fn new(title: Vec<LibCourse>, library: Library) -> Self {
        let seed = (js_sys::Math::random() * 9.0e15) as u64 | 1;
        // Course World's setting (the Mods page's): TGRCode unless the library was picked (the
        // course server is the library here)
        let setting = storage()
            .and_then(|s| {
                s.get_item(&format!(
                    "{STORAGE_PREFIX}{}",
                    mmk_front::mods::COURSE_WORLD_KEY
                ))
                .ok()
                .flatten()
            })
            .map(|v| v.trim().trim_matches('"').to_string());
        let tgr = match setting.as_deref() {
            Some("library" | "server") => None,
            _ => Some(Tgr::default()),
        };
        Self {
            input: 0,
            audio: None,
            maker_sounds: None,
            title,
            library,
            slots: vec![None; COURSEBOT_SLOTS],
            keys: None,
            world_keys: None,
            tgr,
            list: Rc::default(),
            list_request: Rc::default(),
            thumbs: Rc::default(),
            replies: Rc::default(),
            music_wanted: None,
            edit_wanted: None,
            edit_track: None,
            rng: seed,
        }
    }

    /// Put a Coursebot course read from the page's storage into its slot (and the library).
    pub fn restore_slot(&mut self, slot: usize, bytes: &[u8]) {
        if slot >= COURSEBOT_SLOTS {
            return;
        }
        match front_course::decode(bytes, || Err("not course data".into())) {
            Ok((_, data, _)) => {
                if let Ok(c) = LibCourse::new(slot_id(slot), &stem_of(slot), false, &data, None) {
                    keep(&self.library, c);
                }
                self.slots[slot] = Some(data);
            }
            Err(e) => crate::app::log(&format!("Coursebot slot {}: {e}", slot + 1)),
        }
    }

    /// The Course Maker's files arrived: its music again (it found nothing to play before).
    pub fn edit_files_read(&mut self) {
        if let Some((style, theme)) = self.edit_track.clone()
            && let Some(d) = self.maker_director()
        {
            d.edit_music(&style, theme);
        }
    }

    fn maker_director(&mut self) -> Option<&mut mmk_host::audio::Director> {
        if self.maker_sounds.is_none() {
            self.maker_sounds = self.audio.as_ref().map(|a| a.director());
        }
        self.maker_sounds.as_mut()
    }

    fn load(&mut self, game: &mut Game, c: &LibCourse) -> Result<LoadedCourse, String> {
        let data = c.raw()?;
        self.load_data(game, &data, Some(c.entry.key.clone()))
    }

    fn load_data(
        &mut self,
        game: &mut Game,
        data: &[u8],
        key: Option<String>,
    ) -> Result<LoadedCourse, String> {
        let course = mmk_formats::smm2::Course::parse(data).map_err(|e| e.to_string())?;
        let loaded = LoadedCourse {
            name: course.header.name.to_string_lossy(),
            key,
            theme: course.main.header.theme,
        };
        game.set_course(Some(course));
        game.session.set_audio(self.audio.clone());
        let level = &game.session.world.level;
        self.music_wanted = Some((level.style.clone(), level.theme));
        Ok(loaded)
    }

    /// The plain course data of a Course World course the client has (or "on its way").
    fn server_course(&self, code: &str) -> Result<Vec<u8>, String> {
        let tgr = self
            .tgr
            .as_ref()
            .ok_or("Course World is set to the library")?;
        let raw = tgr.now(|c| c.data(code)).map_err(|e| match e {
            // (not fetched yet: `prefetch`)
            tgrcode::TgrError::Failed(_) => {
                "the course is still downloading from TGRCode: try again in a moment".to_string()
            }
            e => e.to_string(),
        })?;
        course_body(&raw, self.keys.as_deref())
    }

    /// Start fetching a Course World course's file (the client keeps it).
    fn prefetch(&self, code: &str) {
        let Some(tgr) = self.tgr.clone() else { return };
        let code = code.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = tgr.run(|c| c.data(&code)).await;
        });
    }
}

/// TGRCode's course file → the plain course data (decrypted with the course key table).
fn course_body(raw: &[u8], keys: Option<&[u8]>) -> Result<Vec<u8>, String> {
    if raw.len() == mmk_formats::smm2::COURSE_SIZE {
        return Ok(raw.to_vec());
    }
    let keys = keys.ok_or_else(|| mmk_host::course::no_key_table("no game data imported"))?;
    let table = mmk_formats::smm2::crypto::table_from_bytes(keys).map_err(|e| e.to_string())?;
    tgrcode::body(raw, &table).map_err(|e| e.to_string())
}

/// A Super World answer with its map decrypted with the world-map key table, as hex (as
/// `mmk_online::world::Backend::super_world` hands it over).
fn super_world_map(mut v: Value, keys: Option<&[u8]>) -> Result<Value, String> {
    let blob = v["map"]
        .as_str()
        .and_then(mmk_online::base64)
        .ok_or("TGRCode sent no world map")?;
    let keys = keys.ok_or("the game data has no world-map key table: import it again")?;
    let table = mmk_formats::smm2::crypto::table_from_bytes(keys).map_err(|e| e.to_string())?;
    let map = mmk_formats::smm2::world::decrypt_blob(&blob, &table, true)
        .map_err(|e| format!("the world map: {e}"))?;
    v["map"] = Value::String(mmk_modes::superworld::to_hex(&map));
    Ok(v)
}

/// The library id of Coursebot slot `slot`.
fn slot_id(slot: usize) -> String {
    format!("coursebot:{:03}", slot + 1)
}

fn stem_of(slot: usize) -> String {
    format!("course_{:03}", slot + 1)
}

fn slot_of(id: &str) -> Option<usize> {
    let n: usize = id.strip_prefix("coursebot:")?.parse().ok()?;
    (1..=COURSEBOT_SLOTS).contains(&n).then(|| n - 1)
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

const STORAGE_PREFIX: &str = "mmk-front:";

/// The host for one tick: the game and the data.
pub struct ShellHost<'a> {
    pub game: &'a mut Game,
    pub data: &'a mut HostData,
}

impl Host for ShellHost<'_> {
    fn library(&mut self) -> Vec<CourseEntry> {
        let mut v: Vec<CourseEntry> = self
            .data
            .library
            .borrow()
            .iter()
            .map(|c| c.entry.clone())
            .collect();
        v.sort_by(|a, b| front_course::natural(&a.name, &b.name));
        v
    }

    fn request_server_courses(&mut self) -> bool {
        self.request_world_list("popular")
    }

    fn poll_server_courses(&mut self) -> Option<Vec<WorldCourse>> {
        self.data.list.borrow_mut().take()
    }

    fn course_world(&self) -> &str {
        if self.data.tgr.is_some() {
            "tgrcode"
        } else {
            "library"
        }
    }

    fn world_lists_by_tab(&self) -> bool {
        self.data.tgr.is_some()
    }

    fn request_world_list(&mut self, tab: &str) -> bool {
        let Some(tgr) = self.data.tgr.clone() else {
            return false;
        };
        let n = self.data.list_request.get() + 1;
        self.data.list_request.set(n);
        let (list, current) = (self.data.list.clone(), self.data.list_request.clone());
        let tab = tab.to_string();
        let limit = if tab == "new" { 20 } else { 30 };
        wasm_bindgen_futures::spawn_local(async move {
            let args = json!({ "sort": tab, "limit": limit });
            let found = match tgr.run(|c| tgrcode::call(c, "search", &args)).await {
                Ok(v) => v
                    .as_array()
                    .map(|a| a.iter().filter_map(WorldCourse::server).collect())
                    .unwrap_or_default(),
                Err(e) => {
                    crate::app::log(&format!("[course world] {tab}: {e}"));
                    Vec::new()
                }
            };
            if current.get() == n {
                *list.borrow_mut() = Some(found);
            }
        });
        true
    }

    fn request_thumbnail(&mut self, code: &str, entire: bool) -> bool {
        let Some(tgr) = self.data.tgr.clone() else {
            return false;
        };
        let (thumbs, code) = (self.data.thumbs.clone(), code.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            let jpeg = tgr
                .run(|c| c.thumbnail(&code, entire))
                .await
                .unwrap_or_default();
            thumbs.borrow_mut().push_back((code, entire, jpeg));
        });
        true
    }

    fn poll_thumbnail(&mut self) -> Option<(String, bool, Vec<u8>)> {
        self.data.thumbs.borrow_mut().pop_front()
    }

    fn prefetch_course(&mut self, source: &Source) {
        if let Source::Server(code) = source {
            self.data.prefetch(code);
        }
    }

    fn load_course(&mut self, source: &Source) -> Result<LoadedCourse, String> {
        match source {
            Source::Library(id) => {
                let library = self.data.library.clone();
                let lib = library.borrow();
                let c = lib
                    .iter()
                    .find(|c| &c.entry.id == id)
                    .ok_or_else(|| format!("no course {id}"))?;
                self.data.load(self.game, c)
            }
            Source::Server(code) => {
                // the course page fetched its file when it opened; if it hasn't arrived yet,
                // it is fetched now and the next try plays it
                let data = self.data.server_course(code).inspect_err(|_| {
                    self.data.prefetch(code);
                })?;
                let key = format!("{:016x}", front_course::fnv1a64(&data));
                self.data.load_data(self.game, &data, Some(key))
            }
        }
    }

    fn title_courses(&mut self) -> usize {
        self.data.title.len()
    }

    fn load_title_course(&mut self, index: usize) -> bool {
        if index >= self.data.title.len() {
            return false;
        }
        let c = self.data.title.remove(index);
        let ok = self.data.load(self.game, &c).is_ok();
        self.data.title.insert(index, c);
        ok
    }

    fn unload_course(&mut self) {
        if let Some(a) = &self.data.audio {
            a.send(mmk_audio::Command::StopAll);
        }
        self.game.session.set_audio(None);
        self.game.set_course(None);
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
        self.game.session.world.main_char = (character as usize).min(3);
    }

    fn set_run_counters(&mut self, score: u32, coins: u32, lives: u32) {
        self.game
            .session
            .world
            .set_run_counters(score, coins, lives);
    }

    fn local_offset(&self, secs: i64) -> i64 {
        let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(secs as f64 * 1000.0));
        // minutes UTC is ahead of local time
        -(d.get_timezone_offset() * 60.0) as i64
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

    fn online(&mut self, id: u32, action: &str, args: &Value, _token: Option<&str>) -> bool {
        // (uploads go to an own course server, which the browser doesn't reach)
        let Some(tgr) = self.data.tgr.clone() else {
            return false;
        };
        if action == "upload" {
            return false;
        }
        let replies = self.data.replies.clone();
        let (action, args) = (action.to_string(), args.clone());
        let (keys, world_keys) = (self.data.keys.clone(), self.data.world_keys.clone());
        let library = self.data.library.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let code = args["code"].as_str().unwrap_or_default().to_string();
            let answer = match action.as_str() {
                // kept in the library (and the page's storage) from now on
                "download" => match tgr.run(|c| c.data(&code)).await {
                    Ok(raw) => course_body(&raw, keys.as_deref()).and_then(|body| {
                        let id = format!("download:{code}");
                        let c = LibCourse::new(id, &code, false, &body, None)?;
                        let file = format!("{DOWNLOADS}/{code}.course");
                        write_save(file.clone(), c.packed().to_vec());
                        keep(&library, c);
                        Ok(json!({ "file": file }))
                    }),
                    Err(e) => Err(e.to_string()),
                },
                "super_world" => tgr
                    .run(|c| tgrcode::call(c, "super_world", &args))
                    .await
                    .and_then(|v| super_world_map(v, world_keys.as_deref())),
                _ => tgr.run(|c| tgrcode::call(c, &action, &args)).await,
            };
            replies.borrow_mut().push_back((id, answer));
        });
        true
    }

    fn poll_online(&mut self) -> Option<(u32, Result<Value, String>)> {
        self.data.replies.borrow_mut().pop_front()
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
        // the page reads the maker's files when its style or theme changes (and after a test
        // play, whose course's files replace them)
        let track = track.map(|(s, t)| (s.to_string(), t));
        if track.is_some() && track != self.data.edit_track {
            self.data.edit_wanted = track.clone();
        }
        self.data.edit_track = track.clone();
        let track = track.as_ref().map(|(s, t)| (s.as_str(), *t));
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
        let level = &self.game.session.world.level;
        self.data.music_wanted = Some((level.style.clone(), level.theme));
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
        // over an entry: its slot; else the first free slot
        if let Some(slot) = id.and_then(slot_of) {
            return self.save_slot(slot, course);
        }
        let free = self
            .data
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or("the Coursebot is full")?;
        self.save_slot(free, course)
    }

    fn coursebot_slots(&mut self) -> Vec<Option<CourseEntry>> {
        let lib = self.data.library.borrow();
        (0..COURSEBOT_SLOTS)
            .map(|slot| {
                let id = slot_id(slot);
                lib.iter()
                    .find(|c| c.entry.id == id)
                    .map(|c| c.entry.clone())
            })
            .collect()
    }

    fn save_slot(&mut self, slot: usize, course: &[u8]) -> Result<String, String> {
        if slot >= COURSEBOT_SLOTS {
            return Err(format!("no Coursebot slot {}", slot + 1));
        }
        let c = LibCourse::new(slot_id(slot), &stem_of(slot), false, course, None)?;
        let id = c.entry.id.clone();
        write_save(slot_file(slot), c.packed().to_vec());
        keep(&self.data.library, c);
        self.data.slots[slot] = Some(course.to_vec());
        Ok(id)
    }

    fn delete_slot(&mut self, slot: usize) -> bool {
        if self
            .data
            .slots
            .get_mut(slot)
            .and_then(Option::take)
            .is_none()
        {
            return false;
        }
        let id = slot_id(slot);
        self.data.library.borrow_mut().retain(|c| c.entry.id != id);
        remove_save(slot_file(slot));
        true
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
        match source {
            Source::Library(id) => {
                let lib = self.data.library.borrow();
                let c = lib
                    .iter()
                    .find(|c| &c.entry.id == id)
                    .ok_or_else(|| format!("no course {id}"))?;
                c.raw()
            }
            Source::Server(code) => self.data.server_course(code).inspect_err(|_| {
                self.data.prefetch(code);
            }),
        }
    }

    fn part_icon(&mut self, style: &str, part: i16, flags: u32) -> Option<mmk_front::Icon> {
        let icon = self.game.part_icon(style, part, flags);
        // art still being read: no picture yet (asked again later)
        if crate::web::art_loading(&self.game.cache) {
            return None;
        }
        icon
    }

    fn art_loading(&self) -> bool {
        crate::web::art_loading(&self.game.cache)
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
        if source == "tgrcode" {
            if self.data.tgr.is_none() {
                self.data.tgr = Some(Tgr::default());
            }
        } else {
            self.data.tgr = None;
        }
        true
    }

    fn load_value(&mut self, key: &str) -> Option<String> {
        storage()?
            .get_item(&format!("{STORAGE_PREFIX}{key}"))
            .ok()
            .flatten()
    }

    fn store_value(&mut self, key: &str, value: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item(&format!("{STORAGE_PREFIX}{key}"), value);
        }
    }

    fn random(&mut self) -> u64 {
        let mut x = self.data.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.data.rng = x;
        x
    }
}

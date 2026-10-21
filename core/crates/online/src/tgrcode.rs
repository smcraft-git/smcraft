//! Course World through TGRCode (`https://tgrcode.com/mm2`, TheGreatRambler's third-party API
//! over Nintendo's Course World; source github.com/TheGreatRambler/MariOver). This client never
//! talks to Nintendo itself: TGRCode does, and hands back JSON, the encrypted course files and
//! the thumbnails. It is read-only (no likes, comments, uploads or clears).
//!
//! The endpoints used (all GET, verified against the live service on 2026-10-21):
//!
//! | path | answer |
//! |---|---|
//! | `level_info/<code>` | a course record (below); 400 `{"error"}` for a bad or unknown code |
//! | `level_data/<code>` | the course file: an encrypted `.bcd` (0x5C000 bytes) |
//! | `level_thumbnail/<code>` | the game's thumbnail block (a 640 × 360 JPEG, zero padded) |
//! | `level_entire_thumbnail/<code>` | the whole-course map, a JPEG |
//! | `level_comments/<code>` | `{"comments": [{text?, posted, type, poster: user}]}` |
//! | `user_info/<maker id>` | a user record (name, code, maker_points, …) |
//! | `get_posted/<maker id>` (also `get_liked`, `get_played`, `get_first_cleared`, `get_world_record`) | `{"courses": [record]}` (up to 100) |
//! | `search_popular?count=1..100&difficulty=e\|n\|ex\|sex` | `{"courses": [record]}` by course points |
//! | `search_new?count=1..20` | the newest uploads |
//! | `search_endless_mode?count=1..300&difficulty=…` | a draw from the Endless Challenge pool |
//! | `super_world/<id>` | a Super World: `{worlds, levels, planet_type, unk1, courses: [record], uploader}`; `unk1` is the encrypted world map (0x500 bytes, base64) |
//! | `get_super_worlds` | `{"super_worlds": [{id, worlds, levels, planet_type, uploader}]}`: 50 picked up |
//!
//! A course record: `course_id` (9 characters, no dashes), `data_id`, `name`, `description`,
//! `uploaded` (seconds since 1970), `game_style` (0 SMB1, 1 SMB3, 2 SMW, 3 NSMBU, 4 SM3DW),
//! `theme`, `difficulty` (0–3), `tags`, `plays`, `attempts`, `clears`, `likes`, `boos`,
//! `world_record` (ms, missing when nobody cleared), `num_comments`, `clear_condition`,
//! `uploader` / `record_holder` / `first_completer` (user records: `name`, `code`, …).
//!
//! The service limits request rates (nginx answers 429 to bursts), so this client is polite:
//! one request at a time in the whole process, [`MIN_INTERVAL`] apart, a busy answer waited out
//! a few times, and every answer cached (in memory, and on disk when a folder is given): lists
//! for minutes (an Endless draw not at all), course and user records for half an hour, course
//! files and thumbnails for good.
//! When the service can't be reached, a cached answer of any age is used (offline play of what
//! was seen before).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use mmk_formats::smm2::crypto::{self, BCD_SIZE};
use mmk_formats::smm2::{COURSE_SIZE, KeyTable};
use serde_json::{Value, json};

use crate::Listing;

pub const BASE: &str = "https://tgrcode.com/mm2";
/// Between two requests to the service (the whole process).
pub const MIN_INTERVAL: Duration = Duration::from_millis(2000);
/// A busy answer is asked again this many times (waiting its Retry-After, else 5 s, doubling).
pub const RETRIES: u32 = 3;

/// The largest thumbnail block the game takes (the JPEG, then its size and signature).
const MAX_THUMBNAIL: usize = 0x1bf9c;

/// Why a request failed.
#[derive(Clone, Debug, PartialEq)]
pub enum TgrError {
    /// Too many requests (HTTP 429 / 503): the wait the service asked for, if any.
    Busy(Option<Duration>),
    /// The service's own answer: no such course / maker, a bad code (its `error` text).
    NotFound(String),
    /// The network, an unexpected status, an answer that isn't what was asked for.
    Failed(String),
}

impl std::fmt::Display for TgrError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TgrError::Busy(_) => write!(f, "TGRCode is busy, try again in a moment"),
            TgrError::NotFound(e) => write!(f, "{e}"),
            TgrError::Failed(e) => write!(f, "TGRCode: {e}"),
        }
    }
}

pub type TgrResult<T> = std::result::Result<T, TgrError>;

/// How requests reach the service (HTTP, or a stand-in in tests).
pub trait Fetch: Send + Sync {
    /// GET `path` (e.g. `/level_info/ABCDEFGHJ`): the body of a 200 answer.
    fn get(&self, path: &str) -> TgrResult<Vec<u8>>;
}

/// The real service over HTTPS.
pub struct Http {
    base: String,
    agent: ureq::Agent,
}

impl Http {
    pub fn new(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(40)))
            .user_agent("SMCraft (open-source Super Mario Maker 2 rebuild; Course World)")
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent,
        }
    }
}

impl Fetch for Http {
    fn get(&self, path: &str) -> TgrResult<Vec<u8>> {
        let url = format!("{}{path}", self.base);
        let failed = |e: String| TgrError::Failed(format!("{path}: {e}"));
        let mut resp = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| failed(e.to_string()))?;
        let status = resp.status().as_u16();
        if status == 429 || status == 503 {
            let after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok())
                .map(Duration::from_secs);
            return Err(TgrError::Busy(after));
        }
        let body = resp
            .body_mut()
            .with_config()
            .limit(8 << 20)
            .read_to_vec()
            .map_err(|e| failed(e.to_string()))?;
        match status {
            200 => Ok(body),
            400 | 404 => Err(TgrError::NotFound(
                serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|v| v["error"].as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("HTTP {status}")),
            )),
            st => Err(failed(format!("HTTP {st}"))),
        }
    }
}

/// The last request's time, process-wide: one request at a time, [`MIN_INTERVAL`] apart.
static PACE: Mutex<Option<Instant>> = Mutex::new(None);

/// How long an answer stays fresh.
fn fresh_for(path: &str) -> Option<Duration> {
    if path.starts_with("/level_data/") || path.contains("thumbnail/") {
        None // (never changes)
    } else if path.starts_with("/search_endless_mode") {
        // (a random draw each time: asked anew, kept for when the service can't be reached)
        Some(Duration::ZERO)
    } else if path.starts_with("/search_") {
        Some(Duration::from_secs(10 * 60))
    } else {
        Some(Duration::from_secs(30 * 60))
    }
}

/// The TGRCode client (share it with an `Arc`; calls block, run them off the UI thread).
pub struct TgrCode {
    fetch: Box<dyn Fetch>,
    /// On-disk cache folder (None: memory only).
    cache: Option<PathBuf>,
    memory: Mutex<HashMap<String, (SystemTime, Vec<u8>)>>,
    interval: Duration,
    backoff: Duration,
}

impl TgrCode {
    /// The real service, caching in `cache` (created when needed).
    pub fn new(cache: Option<PathBuf>) -> Self {
        Self::with_fetch(Box::new(Http::new(BASE)), cache, MIN_INTERVAL)
    }

    pub fn with_fetch(fetch: Box<dyn Fetch>, cache: Option<PathBuf>, interval: Duration) -> Self {
        Self {
            fetch,
            cache,
            memory: Mutex::new(HashMap::new()),
            interval,
            backoff: if interval.is_zero() {
                Duration::ZERO
            } else {
                Duration::from_secs(5)
            },
        }
    }

    fn cache_file(&self, path: &str) -> Option<PathBuf> {
        let name: String = path
            .trim_start_matches('/')
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        self.cache.as_ref().map(|d| d.join(name))
    }

    /// A cached answer: (when it was stored, bytes).
    fn cached(&self, path: &str) -> Option<(SystemTime, Vec<u8>)> {
        if let Some(e) = self.memory.lock().unwrap().get(path) {
            return Some(e.clone());
        }
        let f = self.cache_file(path)?;
        let when = std::fs::metadata(&f).and_then(|m| m.modified()).ok()?;
        let bytes = std::fs::read(&f).ok()?;
        Some((when, bytes))
    }

    fn keep(&self, path: &str, bytes: &[u8]) {
        let now = SystemTime::now();
        self.memory
            .lock()
            .unwrap()
            .insert(path.to_string(), (now, bytes.to_vec()));
        if let Some(f) = self.cache_file(path) {
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&f, bytes);
        }
    }

    /// GET `path`: a fresh cached answer, else the service (paced, a busy answer retried), else
    /// a stale cached answer when the service can't be reached.
    pub fn get(&self, path: &str) -> TgrResult<Vec<u8>> {
        let cached = self.cached(path);
        if let Some((when, bytes)) = &cached {
            let age = when.elapsed().unwrap_or_default();
            if fresh_for(path).is_none_or(|ttl| age < ttl) {
                return Ok(bytes.clone());
            }
        }
        let mut wait = self.backoff;
        let mut r = Err(TgrError::Busy(None));
        for attempt in 0..=RETRIES {
            r = {
                let mut last = PACE.lock().unwrap();
                if let Some(t) = *last {
                    let left = self.interval.saturating_sub(t.elapsed());
                    if !left.is_zero() {
                        std::thread::sleep(left);
                    }
                }
                let r = self.fetch.get(path);
                *last = Some(Instant::now());
                r
            };
            match &r {
                Err(TgrError::Busy(after)) if attempt < RETRIES => {
                    std::thread::sleep(after.unwrap_or(wait).min(Duration::from_secs(30)));
                    wait *= 2;
                }
                _ => break,
            }
        }
        match r {
            Ok(b) => {
                self.keep(path, &b);
                Ok(b)
            }
            // (offline, or the service down: what was seen before)
            Err(TgrError::Failed(_) | TgrError::Busy(_)) if cached.is_some() => {
                Ok(cached.unwrap().1)
            }
            Err(e) => Err(e),
        }
    }

    fn json(&self, path: &str) -> TgrResult<Value> {
        let b = self.get(path)?;
        let v: Value = serde_json::from_slice(&b)
            .map_err(|e| TgrError::Failed(format!("{path}: not JSON ({e})")))?;
        match v["error"].as_str() {
            Some(e) => Err(TgrError::NotFound(e.to_string())),
            None => Ok(v),
        }
    }

    fn courses(&self, path: &str) -> TgrResult<Vec<Value>> {
        let v = self.json(path)?;
        v["courses"]
            .as_array()
            .cloned()
            .ok_or_else(|| TgrError::Failed(format!("{path}: no course list")))
    }

    /// A course record by its code (dashes optional).
    pub fn course(&self, code: &str) -> TgrResult<Value> {
        let (raw, _) = checked(code, Some(false))?;
        self.json(&format!("/level_info/{raw}"))
    }

    /// A list of course records.
    pub fn list(&self, list: &TgrList, count: usize) -> TgrResult<Vec<Value>> {
        let path = match list {
            TgrList::Popular(d) => format!(
                "/search_popular?count={}&difficulty={}",
                count.clamp(1, 100),
                d.code()
            ),
            TgrList::New => format!("/search_new?count={}", count.clamp(1, 20)),
            TgrList::Endless(d) => format!(
                "/search_endless_mode?count={}&difficulty={}",
                count.clamp(1, 300),
                d.code()
            ),
            TgrList::Maker(kind, id) => {
                let (raw, _) = checked(id, Some(true))?;
                format!("/{}/{raw}", kind.endpoint())
            }
        };
        self.courses(&path)
    }

    /// A user record by maker ID.
    pub fn maker(&self, id: &str) -> TgrResult<Value> {
        let (raw, _) = checked(id, Some(true))?;
        self.json(&format!("/user_info/{raw}"))
    }

    /// A course's comments (TGRCode's records, newest first as the service gives them).
    pub fn comments(&self, code: &str) -> TgrResult<Vec<Value>> {
        let (raw, _) = checked(code, Some(false))?;
        let v = self.json(&format!("/level_comments/{raw}"))?;
        Ok(v["comments"].as_array().cloned().unwrap_or_default())
    }

    /// The course file as the service gives it (the encrypted `.bcd`).
    pub fn data(&self, code: &str) -> TgrResult<Vec<u8>> {
        let (raw, _) = checked(code, Some(false))?;
        self.get(&format!("/level_data/{raw}"))
    }

    /// The plain course data (0x5BFC0 bytes), decrypted with the asset pack's course key table.
    pub fn course_body(&self, code: &str, table: &KeyTable) -> TgrResult<Vec<u8>> {
        body(&self.data(code)?, table)
    }

    /// A course's thumbnail (`entire`: the whole-course map) as a JPEG.
    pub fn thumbnail(&self, code: &str, entire: bool) -> TgrResult<Vec<u8>> {
        let (raw, _) = checked(code, Some(false))?;
        let kind = if entire {
            "level_entire_thumbnail"
        } else {
            "level_thumbnail"
        };
        let b = self.get(&format!("/{kind}/{raw}"))?;
        thumbnail_jpeg(&b)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| TgrError::Failed(format!("{code}: the thumbnail isn't a JPEG")))
    }

    /// A Super World by its id (`<hex>_<digits>`, from a user record's `super_world_id`).
    pub fn super_world(&self, id: &str) -> TgrResult<Value> {
        if id.len() < 34
            || !id
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '_' || c.is_ascii_digit())
        {
            return Err(TgrError::NotFound(format!("{id}: not a Super World ID")));
        }
        self.json(&format!("/super_world/{id}"))
    }

    /// Super Worlds picked up by the service (no maps or courses: fetch each by its id).
    pub fn super_worlds(&self) -> TgrResult<Vec<Value>> {
        let v = self.json("/get_super_worlds")?;
        Ok(v["super_worlds"].as_array().cloned().unwrap_or_default())
    }
}

/// Endless / Popular difficulty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Easy,
    Normal,
    Expert,
    SuperExpert,
}

impl Level {
    fn code(self) -> &'static str {
        match self {
            Level::Easy => "e",
            Level::Normal => "n",
            Level::Expert => "ex",
            Level::SuperExpert => "sex",
        }
    }

    /// `easy`, `normal`, `expert`, `super_expert` (as `mmk_modes::endless::Difficulty::name`).
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "easy" | "e" => Level::Easy,
            "normal" | "n" => Level::Normal,
            "expert" | "ex" => Level::Expert,
            "super_expert" | "sex" => Level::SuperExpert,
            _ => return None,
        })
    }
}

/// A maker's course lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MakerList {
    Posted,
    Liked,
    Played,
    FirstCleared,
    Records,
}

impl MakerList {
    fn endpoint(self) -> &'static str {
        match self {
            MakerList::Posted => "get_posted",
            MakerList::Liked => "get_liked",
            MakerList::Played => "get_played",
            MakerList::FirstCleared => "get_first_cleared",
            MakerList::Records => "get_world_record",
        }
    }

    /// `posted`, `liked`, `played`, `first_clears`, `records`.
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "posted" | "" => MakerList::Posted,
            "liked" => MakerList::Liked,
            "played" => MakerList::Played,
            "first_clears" => MakerList::FirstCleared,
            "records" => MakerList::Records,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TgrList {
    Popular(Level),
    New,
    Endless(Level),
    /// A maker's list, by maker ID.
    Maker(MakerList, String),
}

/// A code as typed (dashes, spaces, any case) → (9 characters, is a maker ID), checked with the
/// game's own checksum. `want_maker`: Some(true) only maker IDs, Some(false) only course IDs.
pub fn checked(code: &str, want_maker: Option<bool>) -> TgrResult<(String, bool)> {
    let raw: String = code
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let bad = || TgrError::NotFound(format!("{code}: not a valid ID"));
    let (_, maker) = mmk_formats::course_id::smm2_code_to_data_id(&raw).map_err(|_| bad())?;
    match want_maker {
        Some(true) if !maker => Err(TgrError::NotFound(format!("{code} is a course ID"))),
        Some(false) if maker => Err(TgrError::NotFound(format!("{code} is a maker ID"))),
        _ => Ok((raw, maker)),
    }
}

/// `ABCDEFGHJ` → `ABC-DEF-GHJ` (other lengths as they are).
pub fn dashed(raw: &str) -> String {
    if raw.len() == 9 && raw.is_ascii() {
        format!("{}-{}-{}", &raw[..3], &raw[3..6], &raw[6..])
    } else {
        raw.to_string()
    }
}

/// TGRCode's `game_style` → the game style code ("M1", "M3", "MW", "WU", "3W").
pub fn style_of(n: u64) -> &'static str {
    match n {
        0 => "M1",
        1 => "M3",
        2 => "MW",
        3 => "WU",
        _ => "3W",
    }
}

/// The plain course data of a downloaded file (a `.bcd`, or already plain).
pub fn body(raw: &[u8], table: &KeyTable) -> TgrResult<Vec<u8>> {
    match raw.len() {
        COURSE_SIZE => Ok(raw.to_vec()),
        BCD_SIZE => crypto::decrypt_bcd(raw, table, true)
            .map(|b| b.body)
            .map_err(|e| TgrError::Failed(format!("the course file didn't decrypt ({e})"))),
        n => Err(TgrError::Failed(format!("not a course file ({n} bytes)"))),
    }
}

/// The JPEG in a thumbnail: a plain JPEG, or the game's block (the JPEG, zero padding, its size
/// and signature), up to its end-of-image marker.
pub fn thumbnail_jpeg(b: &[u8]) -> Option<&[u8]> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let area = &b[..b.len().min(MAX_THUMBNAIL)];
    let end = area.windows(2).rposition(|w| w == [0xFF, 0xD9])? + 2;
    Some(&b[..end])
}

fn user_code(u: &Value) -> String {
    u["code"].as_str().map(dashed).unwrap_or_default()
}

/// A course record → a [`Listing`] (codes with dashes, the game style code, the 0–100 difficulty
/// value from the clear rate as the course server's).
pub fn listing(v: &Value) -> Option<Listing> {
    let id = v["course_id"].as_str()?;
    if id.len() != 9 || !id.is_ascii() {
        return None;
    }
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let n = |k: &str| v[k].as_u64().unwrap_or(0);
    let (attempts, clears) = (n("attempts"), n("clears"));
    Some(Listing {
        code: dashed(id),
        name: s("name"),
        maker: v["uploader"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        style: style_of(n("game_style")).to_string(),
        theme: n("theme") as u8,
        plays: n("plays"),
        clears,
        likes: n("likes"),
        boos: n("boos"),
        difficulty: (clears * 100)
            .checked_div(attempts)
            .map_or(50, |r| (100 - r.min(100)) as u8),
        maker_id: user_code(&v["uploader"]),
        record_ms: n("world_record"),
        record_holder: user_code(&v["record_holder"]),
        record_verified: false,
        first_clear: user_code(&v["first_completer"]),
        description: s("description"),
        uploaded: n("uploaded"),
        attempts,
    })
}

/// A course record as the front ends' listing JSON ([`crate::listing_json`]) plus what TGRCode
/// adds: the record holder's and first clearer's names, tags, the comment count, the source.
pub fn listing_json(v: &Value) -> Option<Value> {
    let l = listing(v)?;
    let mut j = crate::listing_json(&l);
    let name = |k: &str| v[k]["name"].as_str().unwrap_or_default().to_string();
    j["record_holder_name"] = json!(name("record_holder"));
    j["first_clear_name"] = json!(name("first_completer"));
    j["tags"] = json!(
        v["tags"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
            .unwrap_or_default()
    );
    j["comments"] = json!(v["num_comments"].as_u64().unwrap_or(0));
    j["source"] = json!("tgrcode");
    Some(j)
}

/// A user record → the front ends' account shape plus TGRCode's statistics.
pub fn maker_json(u: &Value) -> Value {
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    json!({
        "id": 0, "name": u["name"].as_str().unwrap_or_default(), "maker_id": user_code(u),
        "maker_points": n("maker_points"), "courses_played": n("courses_played"),
        "courses_cleared": n("courses_cleared"), "first_clears": n("first_clears"),
        "world_records": n("world_records"), "uploaded_levels": n("uploaded_levels"),
        "likes": n("likes"), "super_world_id": u["super_world_id"].as_str().unwrap_or_default(),
    })
}

/// The text comments (TGRCode also lists drawings and reaction images, which have no text).
pub fn comments_json(list: &[Value]) -> Value {
    Value::Array(
        list.iter()
            .filter_map(|c| {
                let text = c["text"].as_str()?;
                Some(json!({
                    "maker_id": user_code(&c["poster"]),
                    "name": c["poster"]["name"].as_str().unwrap_or_default(),
                    "text": text,
                    "posted": c["posted"].as_f64().unwrap_or(0.0) as u64,
                }))
            })
            .collect(),
    )
}

fn courses_json(list: &[Value]) -> Value {
    Value::Array(list.iter().filter_map(listing_json).collect())
}

/// One Course World call by name, the read-only ones of [`crate::call`] (same arguments and
/// answers) plus TGRCode's lists:
///
/// | action | args | result |
/// |---|---|---|
/// | `info` | `{code}` | listing |
/// | `search` | `{sort: hot \| popular \| new \| endless, difficulty, limit}` | `[listing]` |
/// | `comments` | `{code}` | `[{maker_id, name, text, posted}]` |
/// | `maker` | `{maker_id}` | `{maker, courses}` (the uploaded courses) |
/// | `maker_courses` | `{maker_id, kind: posted \| liked \| played \| first_clears \| records}` | `[listing]` |
/// | `super_world` | `{id}` | `{id, worlds, levels, planet_type, created, map, courses: [listing], maker}` (`map`: the encrypted world map, base64) |
/// | `super_worlds` | — | `[{id, worlds, levels, planet_type, maker}]` |
///
/// Hot is a fresh draw from the Endless Challenge pool (TGRCode has no "hot" list); Popular
/// is TGRCode's course-point ranking at `difficulty` (default normal). The rest (likes,
/// comments, clears, accounts) are refused: TGRCode is read-only.
pub fn call(t: &TgrCode, action: &str, args: &Value) -> crate::Result<Value> {
    let s = |k: &str| args[k].as_str().unwrap_or_default();
    let level = Level::from_name(s("difficulty")).unwrap_or(Level::Normal);
    let limit = args["limit"].as_u64().unwrap_or(30) as usize;
    let e = |e: TgrError| e.to_string();
    Ok(match action {
        "info" => listing_json(&t.course(s("code")).map_err(e)?)
            .ok_or_else(|| "TGRCode: not a course record".to_string())?,
        "search" => {
            let list = match s("sort") {
                "new" => TgrList::New,
                "hot" | "endless" => TgrList::Endless(level),
                _ => TgrList::Popular(level),
            };
            courses_json(&t.list(&list, limit).map_err(e)?)
        }
        "comments" => comments_json(&t.comments(s("code")).map_err(e)?),
        "maker" => {
            let id = s("maker_id");
            let u = t.maker(id).map_err(e)?;
            let posted = t
                .list(&TgrList::Maker(MakerList::Posted, id.to_string()), 100)
                .map_err(e)?;
            json!({ "maker": maker_json(&u), "courses": courses_json(&posted) })
        }
        "maker_courses" => {
            let kind = MakerList::from_name(s("kind")).ok_or("unknown list")?;
            courses_json(
                &t.list(&TgrList::Maker(kind, s("maker_id").to_string()), 100)
                    .map_err(e)?,
            )
        }
        "super_world" => {
            let w = t.super_world(s("id")).map_err(e)?;
            json!({
                "id": w["id"], "worlds": w["worlds"], "levels": w["levels"],
                "planet_type": w["planet_type"], "created": w["created"], "map": w["unk1"],
                "courses": courses_json(w["courses"].as_array().map(Vec::as_slice).unwrap_or(&[])),
                "maker": maker_json(&w["uploader"]),
            })
        }
        "super_worlds" => Value::Array(
            t.super_worlds()
                .map_err(e)?
                .iter()
                .map(|w| {
                    json!({
                        "id": w["id"], "worlds": w["worlds"], "levels": w["levels"],
                        "planet_type": w["planet_type"], "maker": maker_json(&w["uploader"]),
                    })
                })
                .collect(),
        ),
        _ => {
            return Err(format!(
                "Course World on TGRCode is read-only ({action} needs our own course server)"
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A stand-in service: answers by path, counts requests, can be busy or down.
    struct Fake {
        answers: HashMap<String, Vec<u8>>,
        requests: Arc<AtomicUsize>,
        busy: Mutex<u32>,
        down: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Fetch for Fake {
        fn get(&self, path: &str) -> TgrResult<Vec<u8>> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            if self.down.load(Ordering::SeqCst) {
                return Err(TgrError::Failed("offline".into()));
            }
            let mut b = self.busy.lock().unwrap();
            if *b > 0 {
                *b -= 1;
                return Err(TgrError::Busy(Some(Duration::ZERO)));
            }
            self.answers
                .get(path)
                .cloned()
                .ok_or_else(|| TgrError::NotFound("No course with that ID".into()))
        }
    }

    /// Made-up records in TGRCode's shape (no real course data).
    fn record(id: &str, name: &str) -> Value {
        json!({
            "course_id": id, "data_id": 1, "name": name, "description": "made up",
            "uploaded": 1_700_000_000u64, "game_style": 2, "theme": 1, "difficulty": 1,
            "tags": [1, 5], "plays": 9, "attempts": 40, "clears": 10, "likes": 3, "boos": 1,
            "world_record": 61_234, "num_comments": 2,
            "uploader": {"name": "Ada", "code": "LMD5PHVPF"},
            "record_holder": {"name": "Bo", "code": "Q8VRKX8MG"},
            "first_completer": {"name": "Cy", "code": "Q8VRKX8MG"},
        })
    }

    /// A made-up key table (tests only; the game's own is never part of the project).
    fn table() -> KeyTable {
        std::array::from_fn(|i| 0x9E37_79B9u32.wrapping_mul(i as u32 + 1) | 0x0101_0101)
    }

    // (valid codes for the made-up records: data ids 5000 and 5001, maker ID of data id 77)
    fn codes() -> (String, String, String) {
        let c = |id, m| mmk_formats::course_id::smm2_data_id_to_code(id, m).replace('-', "");
        (c(5000, false), c(5001, false), c(77, true))
    }

    fn fake() -> (
        TgrCode,
        Arc<AtomicUsize>,
        Arc<std::sync::atomic::AtomicBool>,
    ) {
        let (a, b, m) = codes();
        let mut answers = HashMap::new();
        let mut put = |p: String, v: Value| {
            answers.insert(p, serde_json::to_vec(&v).unwrap());
        };
        put(format!("/level_info/{a}"), record(&a, "Sky"));
        put(
            "/search_new?count=20".into(),
            json!({"courses": [record(&a, "Sky"), record(&b, "Cave")]}),
        );
        put(
            "/search_popular?count=30&difficulty=ex".into(),
            json!({"courses": [record(&b, "Cave")]}),
        );
        put(
            format!("/user_info/{m}"),
            json!({"name": "Ada", "code": m, "maker_points": 12}),
        );
        put(
            format!("/get_posted/{m}"),
            json!({"courses": [record(&a, "Sky")]}),
        );
        put(
            format!("/level_comments/{a}"),
            json!({"comments": [
                {"text": "nice", "posted": 1_700_000_100.5, "type": 1,
                 "poster": {"name": "Bo", "code": "Q8VRKX8MG"}},
                {"type": 0, "posted": 1, "poster": {"name": "Cy"}},
            ]}),
        );
        let body = mmk_formats::smm2::Course::blank("MW").to_bytes().unwrap();
        let bcd = crypto::encrypt_bcd(
            &body,
            &table(),
            crypto::default_header(),
            [7; 16],
            [1, 2, 3, 4],
        )
        .unwrap();
        answers.insert(format!("/level_data/{a}"), bcd);
        let mut block = vec![0xFF, 0xD8, 1, 2, 0xFF, 0xD9];
        block.resize(0x1c000, 0);
        answers.insert(format!("/level_thumbnail/{a}"), block);
        let requests = Arc::new(AtomicUsize::new(0));
        let down = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = Fake {
            answers,
            requests: requests.clone(),
            busy: Mutex::new(0),
            down: down.clone(),
        };
        (
            TgrCode::with_fetch(Box::new(f), None, Duration::ZERO),
            requests,
            down,
        )
    }

    #[test]
    fn records_become_listings() {
        let (a, _, _) = codes();
        let l = listing(&record(&a, "Sky")).unwrap();
        assert_eq!(l.code, dashed(&a));
        assert_eq!((l.style.as_str(), l.theme, l.difficulty), ("MW", 1, 75));
        assert_eq!(
            (l.maker.as_str(), l.maker_id.as_str()),
            ("Ada", "LMD-5PH-VPF")
        );
        assert_eq!(
            (l.record_ms, l.record_holder.as_str()),
            (61_234, "Q8V-RKX-8MG")
        );
        let j = listing_json(&record(&a, "Sky")).unwrap();
        assert_eq!(j["record_holder_name"], "Bo");
        assert_eq!(j["tags"], json!([1, 5]));
        assert!(listing(&json!({"course_id": "short"})).is_none());
    }

    #[test]
    fn ids_are_checked_before_asking() {
        let (a, _, m) = codes();
        assert_eq!(
            checked(&dashed(&a).to_lowercase(), None),
            Ok((a.clone(), false))
        );
        assert_eq!(checked(&m, None), Ok((m.clone(), true)));
        assert!(checked(&m, Some(false)).is_err());
        assert!(checked(&a, Some(true)).is_err());
        assert!(checked("ABC-DEF-GHJ", None).is_err(), "checksum");
        let (t, requests, _) = fake();
        assert!(t.course("nope").is_err());
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_calls_answer_in_the_servers_shapes() {
        let (a, b, m) = codes();
        let (t, requests, _) = fake();
        let info = call(&t, "info", &json!({ "code": dashed(&a) })).unwrap();
        assert_eq!(info["name"], "Sky");
        let new = call(&t, "search", &json!({"sort": "new", "limit": 50})).unwrap();
        assert_eq!(new.as_array().unwrap().len(), 2);
        let pop = call(
            &t,
            "search",
            &json!({"sort": "popular", "difficulty": "expert"}),
        )
        .unwrap();
        assert_eq!(pop[0]["code"], dashed(&b));
        let maker = call(&t, "maker", &json!({ "maker_id": m })).unwrap();
        assert_eq!(maker["maker"]["name"], "Ada");
        assert_eq!(maker["courses"][0]["code"], dashed(&a));
        let c = call(&t, "comments", &json!({ "code": a })).unwrap();
        assert_eq!(c.as_array().unwrap().len(), 1, "only text comments");
        assert_eq!(
            (c[0]["name"].as_str(), c[0]["posted"].as_u64()),
            (Some("Bo"), Some(1_700_000_100))
        );
        assert!(
            call(&t, "vote", &json!({ "code": a }))
                .unwrap_err()
                .contains("read-only")
        );
        // the same calls again: from the cache
        let before = requests.load(Ordering::SeqCst);
        call(&t, "info", &json!({ "code": a })).unwrap();
        call(&t, "search", &json!({"sort": "new"})).unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), before);
        let missing = call(&t, "info", &json!({ "code": dashed(&b) })).unwrap_err();
        assert_eq!(missing, "No course with that ID");
    }

    #[test]
    fn downloads_decrypt_with_the_key_table_and_thumbnails_lose_their_padding() {
        let (a, _, _) = codes();
        let (t, _, _) = fake();
        let body = t.course_body(&a, &table()).unwrap();
        assert_eq!(
            body,
            mmk_formats::smm2::Course::blank("MW").to_bytes().unwrap()
        );
        assert_eq!(
            t.thumbnail(&a, false).unwrap(),
            [0xFF, 0xD8, 1, 2, 0xFF, 0xD9]
        );
        assert!(t.thumbnail(&a, true).is_err());
    }

    #[test]
    fn an_offline_service_answers_from_the_cache() {
        let (a, _, _) = codes();
        let dir = std::env::temp_dir().join(format!("mmk-tgr-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (t, _, _) = fake();
        let TgrCode { fetch, .. } = t;
        let t = TgrCode::with_fetch(fetch, Some(dir.clone()), Duration::ZERO);
        t.course(&a).unwrap();
        // a new client on the same folder, the service down, the record stale: the cache
        let (t2, requests, down) = fake();
        down.store(true, Ordering::SeqCst);
        let TgrCode { fetch, .. } = t2;
        let t2 = TgrCode::with_fetch(fetch, Some(dir.clone()), Duration::ZERO);
        let f = t2.cache_file(&format!("/level_info/{a}")).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(t2.course(&a).unwrap()["name"], "Sky");
        assert_eq!(
            requests.load(Ordering::SeqCst),
            1,
            "asked once, then the cache"
        );
        // nothing cached: the error
        let (t3, _, down) = fake();
        down.store(true, Ordering::SeqCst);
        assert!(t3.course(&a).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_busy_answer_is_waited_out() {
        let (a, _, _) = codes();
        let requests = Arc::new(AtomicUsize::new(0));
        let mut answers = HashMap::new();
        answers.insert(
            format!("/level_info/{a}"),
            serde_json::to_vec(&record(&a, "Sky")).unwrap(),
        );
        let f = Fake {
            answers,
            requests: requests.clone(),
            busy: Mutex::new(2),
            down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let t = TgrCode::with_fetch(Box::new(f), None, Duration::ZERO);
        assert_eq!(t.course(&a).unwrap()["name"], "Sky");
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }

    /// The live service, politely (one request at a time, a few in all). Run by hand:
    /// `cargo test -p mmk-online live_tgrcode -- --ignored` (offline: it says so and passes).
    #[test]
    #[ignore]
    fn live_tgrcode() {
        let dir = std::env::temp_dir().join(format!("mmk-tgr-live-{}", std::process::id()));
        let t = TgrCode::new(Some(dir.clone()));
        let new = match t.list(&TgrList::New, 2) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("offline? {e}");
                return;
            }
        };
        assert!(!new.is_empty());
        let l = listing(&new[0]).unwrap();
        let info = t.course(&l.code).unwrap();
        assert_eq!(info["course_id"].as_str().map(dashed), Some(l.code.clone()));
        let table = std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("smm-work/cache/smm2-3.0.3/keys/course_table.bin"))
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| crypto::table_from_bytes(&b).ok());
        if let Some(table) = table {
            let body = t.course_body(&l.code, &table).unwrap();
            assert!(mmk_formats::smm2::Course::parse(&body).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

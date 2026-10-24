//! Where Course World's courses come from, for the hosts of the front end (the portable shell,
//! the C API): TGRCode (the default), our own course server, or the library alone. The calls
//! block; the hosts run them on threads. Every answer is in the course server's JSON shapes
//! (`listing_json`, [`crate::call`]'s table), so the screens don't care which it is.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

pub use crate::base64;
use crate::tgrcode::{self, TgrCode};

/// The setting's values.
pub const TGRCODE: &str = "tgrcode";
pub const SERVER: &str = "server";
pub const LIBRARY: &str = "library";

#[derive(Clone)]
pub enum Backend {
    /// TGRCode's view of Nintendo's Course World (read-only).
    TgrCode(Arc<TgrCode>),
    /// Our own course server (`mmk server`), by base URL.
    Server(String),
    /// No online courses: Course World lists the library.
    Library,
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Backend::TgrCode(_) => write!(f, "TgrCode"),
            Backend::Server(s) => write!(f, "Server({s})"),
            Backend::Library => write!(f, "Library"),
        }
    }
}

impl Backend {
    /// From the setting (`tgrcode`, `server`, `library`; unknown: TGRCode; none: the server
    /// when a URL is given, else TGRCode) and the server URL (the server setting without a URL:
    /// the library). `cache`: TGRCode's cache folder.
    pub fn from_setting(
        setting: Option<&str>,
        server: Option<&str>,
        cache: Option<PathBuf>,
    ) -> Self {
        let server = server.filter(|s| !s.is_empty());
        let setting = setting.filter(|s| !s.is_empty());
        match (setting, server) {
            (Some(SERVER) | None, Some(s)) => Backend::Server(s.to_string()),
            (Some(SERVER), None) | (Some(LIBRARY), _) => Backend::Library,
            _ => Backend::TgrCode(Arc::new(TgrCode::new(cache))),
        }
    }

    /// The setting's value.
    pub fn name(&self) -> &'static str {
        match self {
            Backend::TgrCode(_) => TGRCODE,
            Backend::Server(_) => SERVER,
            Backend::Library => LIBRARY,
        }
    }

    pub fn online(&self) -> bool {
        !matches!(self, Backend::Library)
    }

    /// The course list's tabs are lists of their own (TGRCode); else one list sorted per tab.
    pub fn lists_by_tab(&self) -> bool {
        matches!(self, Backend::TgrCode(_))
    }

    /// Likes, comments, clears and accounts go nowhere.
    pub fn read_only(&self) -> bool {
        !matches!(self, Backend::Server(_))
    }

    /// The courses of a list tab (`hot`, `popular`, `new`; the server: its popular list for
    /// all), as listing JSON.
    pub fn list(&self, tab: &str) -> crate::Result<Vec<Value>> {
        match self {
            Backend::TgrCode(t) => {
                let limit = if tab == "new" { 20 } else { 30 };
                let v = tgrcode::call(
                    t,
                    "search",
                    &serde_json::json!({ "sort": tab, "limit": limit }),
                )?;
                Ok(v.as_array().cloned().unwrap_or_default())
            }
            Backend::Server(s) => Ok(crate::Client::new(s.clone())
                .search("", None, "popular", 0, 100)?
                .iter()
                .map(crate::listing_json)
                .collect()),
            Backend::Library => Ok(Vec::new()),
        }
    }

    /// A course's data to play or keep: TGRCode's file decrypted with the asset pack's course
    /// key table (`key_table`, read only when needed), or the server's course data.
    pub fn download(
        &self,
        code: &str,
        key_table: impl FnOnce() -> Result<Vec<u8>, String>,
    ) -> crate::Result<Vec<u8>> {
        match self {
            Backend::TgrCode(t) => {
                let raw = t.data(code).map_err(|e| e.to_string())?;
                if raw.len() == mmk_formats::smm2::COURSE_SIZE {
                    return Ok(raw);
                }
                let table = mmk_formats::smm2::crypto::table_from_bytes(&key_table()?)
                    .map_err(|e| e.to_string())?;
                tgrcode::body(&raw, &table).map_err(|e| e.to_string())
            }
            Backend::Server(s) => crate::Client::new(s.clone()).download(code),
            Backend::Library => {
                Err("no online courses (Course World is set to the library)".into())
            }
        }
    }

    /// A Super World to play (TGRCode only): [`tgrcode::call`]'s `super_world` answer with its
    /// map decrypted with the asset pack's world-map key table (`world_table`, read only when
    /// needed) and handed over as hex (`map`), as `mmk_modes::superworld::SuperWorld::from_json`
    /// reads it.
    pub fn super_world(
        &self,
        id: &str,
        world_table: impl FnOnce() -> Result<Vec<u8>, String>,
    ) -> crate::Result<Value> {
        let Backend::TgrCode(t) = self else {
            return Err("Super Worlds come from TGRCode (set Course World to TGRCode)".into());
        };
        let mut v = tgrcode::call(t, "super_world", &serde_json::json!({ "id": id }))?;
        let blob = v["map"]
            .as_str()
            .and_then(base64)
            .ok_or("TGRCode sent no world map")?;
        let table = mmk_formats::smm2::crypto::table_from_bytes(&world_table()?)
            .map_err(|e| e.to_string())?;
        let map = mmk_formats::smm2::world::decrypt_blob(&blob, &table, true)
            .map_err(|e| format!("the world map: {e}"))?;
        v["map"] = Value::String(mmk_modes::superworld::to_hex(&map));
        Ok(v)
    }

    /// A course's thumbnail (`entire`: the whole-course map) as a JPEG.
    pub fn thumbnail(&self, code: &str, entire: bool) -> crate::Result<Vec<u8>> {
        match self {
            Backend::TgrCode(t) => t.thumbnail(code, entire).map_err(|e| e.to_string()),
            Backend::Server(s) => crate::Client::new(s.clone()).thumbnail(code, entire),
            Backend::Library => Err("no thumbnails".into()),
        }
    }

    /// One call of [`crate::call`]'s table (TGRCode: the read-only ones, [`tgrcode::call`]).
    pub fn call(&self, token: Option<&str>, action: &str, args: &Value) -> crate::Result<Value> {
        match self {
            Backend::TgrCode(t) => tgrcode::call(t, action, args),
            Backend::Server(s) => crate::call(s, token, action, args),
            Backend::Library => Err("no course server".into()),
        }
    }

    /// A play / clear report (the server only; best effort).
    pub fn report(&self, code: &str, what: &str) {
        if let Backend::Server(s) = self {
            let _ = crate::Client::new(s.clone()).report(code, what);
        }
    }

    /// The server's URL (replays, ghosts).
    pub fn server(&self) -> Option<&str> {
        match self {
            Backend::Server(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_picks_the_backend() {
        let b = |s: Option<&str>, url: Option<&str>| Backend::from_setting(s, url, None).name();
        assert_eq!(b(None, None), TGRCODE);
        assert_eq!(b(None, Some("http://x")), SERVER);
        assert_eq!(b(Some("tgrcode"), Some("http://x")), TGRCODE);
        assert_eq!(b(Some("server"), Some("http://x")), SERVER);
        assert_eq!(b(Some("server"), Some("")), LIBRARY);
        assert_eq!(b(Some("library"), Some("http://x")), LIBRARY);
        assert!(Backend::from_setting(None, None, None).lists_by_tab());
        assert!(!Backend::Server("http://x".into()).read_only());
    }

    #[test]
    fn base64_decodes() {
        assert_eq!(base64("TWFu").unwrap(), b"Man");
        assert_eq!(base64("TWE=").unwrap(), b"Ma");
        assert_eq!(base64("TQ==").unwrap(), b"M");
        assert_eq!(base64("AAEC/w").unwrap(), vec![0, 1, 2, 255]);
        assert!(base64("a*b").is_none());
        assert!(Backend::Library.super_world("x", || Ok(vec![])).is_err());
    }
}

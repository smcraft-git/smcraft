//! Course World on TGRCode in the browser. TGRCode's API answers pages of any origin (CORS
//! `Access-Control-Allow-Origin: *`), so the page fetches it directly (`mmkNet.get`). The client
//! is the native hosts' (`mmk_online::tgrcode`), whose requests are answered from what the page
//! fetched: a call runs, the paths it lacked are fetched (one request at a time,
//! [`tgrcode::MIN_INTERVAL`] apart, busy answers waited out, as natively; thumbnails last), and
//! it runs again until it has everything. The client keeps the answers in memory as natively (lists for
//! minutes, course files and thumbnails for good); nothing is stored between visits. The runs'
//! pool ([`crate::web_pool`]) asks in the background, behind everything a player waits for.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mmk_online::tgrcode::{self, Fetch, TgrCode, TgrError, TgrResult};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    /// `mmkNet.get(url)`: a promise of `{status, bytes}` (rejects on a network error).
    #[wasm_bindgen(catch, js_namespace = mmkNet, js_name = get)]
    fn net_get(url: &str) -> Result<js_sys::Promise, JsValue>;
}

/// Answers fetched for the client, and the paths it asked for that weren't there.
#[derive(Default)]
struct Pending {
    got: HashMap<String, TgrResult<Vec<u8>>>,
    missing: Vec<String>,
}

/// The client's [`Fetch`]: what the page fetched, else "on its way" (the path noted).
struct PageFetch(Arc<Mutex<Pending>>);

impl Fetch for PageFetch {
    fn get(&self, path: &str) -> TgrResult<Vec<u8>> {
        let mut p = self
            .0
            .lock()
            .map_err(|_| TgrError::Failed("poisoned".into()))?;
        match p.got.get(path) {
            Some(r) => r.clone(),
            None => {
                p.missing.push(path.to_string());
                Err(TgrError::Failed(format!("{path}: on its way")))
            }
        }
    }
}

/// TGRCode's client for the page (clone it into tasks).
#[derive(Clone)]
pub struct Tgr {
    client: Arc<TgrCode>,
    pending: Arc<Mutex<Pending>>,
}

impl Default for Tgr {
    fn default() -> Self {
        let pending = Arc::<Mutex<Pending>>::default();
        let fetch = Box::new(PageFetch(pending.clone()));
        Self {
            client: Arc::new(TgrCode::with_fetch(fetch, None, Duration::ZERO)),
            pending,
        }
    }
}

impl Tgr {
    /// Run `f` on the client with what it has now (e.g. a course file fetched before; a request
    /// it lacks fails as "on its way").
    pub fn now<R>(&self, f: impl FnOnce(&TgrCode) -> R) -> R {
        let r = f(&self.client);
        if let Ok(mut p) = self.pending.lock() {
            p.missing.clear();
        }
        r
    }

    /// Run `f` on the client, fetching what it asks for, until it has it all.
    pub async fn run<R>(&self, f: impl Fn(&TgrCode) -> R) -> R {
        self.run_at(false, f).await
    }

    /// [`Tgr::run`] whose requests wait behind every other (the runs' pool, fetched while the
    /// player looks at other things).
    pub async fn run_background<R>(&self, f: impl Fn(&TgrCode) -> R) -> R {
        self.run_at(true, f).await
    }

    async fn run_at<R>(&self, background: bool, f: impl Fn(&TgrCode) -> R) -> R {
        let mut fetched: Vec<String> = Vec::new();
        let r = loop {
            let r = f(&self.client);
            let missing = match self.pending.lock() {
                Ok(mut p) => std::mem::take(&mut p.missing),
                Err(_) => Vec::new(),
            };
            let mut new: Vec<String> = missing
                .into_iter()
                .filter(|m| !fetched.contains(m))
                .collect();
            new.sort();
            new.dedup();
            if new.is_empty() {
                break r;
            }
            for path in new {
                // already on its way for another call: its answer reaches the client's memory
                if FETCHING.with(|f| f.borrow().contains(&path)) {
                    while FETCHING.with(|f| f.borrow().contains(&path)) {
                        sleep(100.0).await;
                    }
                    fetched.push(path);
                    continue;
                }
                FETCHING.with(|f| f.borrow_mut().push(path.clone()));
                let answer = fetch(&path, background).await;
                FETCHING.with(|f| f.borrow_mut().retain(|p| p != &path));
                if let Ok(mut p) = self.pending.lock() {
                    p.got.insert(path.clone(), answer);
                }
                fetched.push(path);
            }
        };
        // the client kept what it got
        if let Ok(mut p) = self.pending.lock() {
            for path in &fetched {
                p.got.remove(path);
            }
        }
        r
    }
}

thread_local! {
    /// When the next request may go (ms since 1970): one at a time, MIN_INTERVAL apart.
    static NEXT: Cell<f64> = const { Cell::new(0.0) };
    /// Requests waiting for their turn: (rank, ticket); see [`turn`].
    static WAITING: RefCell<Vec<(u8, u64)>> = const { RefCell::new(Vec::new()) };
    static TICKETS: Cell<u64> = const { Cell::new(0) };
    /// Paths being fetched (a second call asking for one waits for the first).
    static FETCHING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Wait for a request's turn: [`tgrcode::MIN_INTERVAL`] after the last one, and no request
/// waiting before it. Thumbnails (a list asks for one per row) wait behind the rest (rank 1), so
/// the course a player opened arrives first; the runs' pool behind those (rank 2).
async fn turn(rank: u8) {
    let me = (rank, TICKETS.get());
    TICKETS.set(me.1 + 1);
    WAITING.with(|w| w.borrow_mut().push(me));
    loop {
        let first = WAITING.with(|w| w.borrow().iter().min().copied()) == Some(me);
        let now = js_sys::Date::now();
        if first && now >= NEXT.get() {
            break;
        }
        sleep(if first { NEXT.get() - now } else { 50.0 }).await;
    }
    WAITING.with(|w| w.borrow_mut().retain(|&x| x != me));
    NEXT.set(js_sys::Date::now() + tgrcode::MIN_INTERVAL.as_millis() as f64);
}

async fn sleep(ms: f64) {
    if ms <= 0.0 {
        return;
    }
    let p = js_sys::Promise::new(&mut |done, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&done, ms as i32);
        }
    });
    let _ = JsFuture::from(p).await;
}

/// GET `path` of the service: in its turn (`background`: behind every other), a busy answer
/// asked again ([`tgrcode::RETRIES`] times, waiting 5 s, doubling: browsers don't show the
/// service's Retry-After).
async fn fetch(path: &str, background: bool) -> TgrResult<Vec<u8>> {
    let rank = if background {
        2
    } else {
        path.contains("thumbnail/") as u8
    };
    let mut wait = 5000.0;
    let mut r = Err(TgrError::Busy(None));
    for attempt in 0..=tgrcode::RETRIES {
        turn(rank).await;
        r = get(path).await;
        match r {
            Err(TgrError::Busy(_)) if attempt < tgrcode::RETRIES => {
                sleep(wait).await;
                wait *= 2.0;
            }
            _ => break,
        }
    }
    r
}

/// One request, its answer as the native client's (`tgrcode::Http`).
async fn get(path: &str) -> TgrResult<Vec<u8>> {
    let failed = |e: String| TgrError::Failed(format!("{path}: {e}"));
    let promise = net_get(&format!("{}{path}", tgrcode::BASE))
        .map_err(|_| failed("the page can't fetch".into()))?;
    // a busy answer (429) comes without the CORS header, so the page sees it as a network
    // error: asked again as a busy one
    let answer = JsFuture::from(promise)
        .await
        .map_err(|_| TgrError::Busy(None))?;
    let status = js_sys::Reflect::get(&answer, &"status".into())
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u16;
    let body = js_sys::Reflect::get(&answer, &"bytes".into())
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok())
        .map(|a| a.to_vec())
        .unwrap_or_default();
    match status {
        200 => Ok(body),
        429 | 503 => Err(TgrError::Busy(None)),
        400 | 404 => Err(TgrError::NotFound(
            serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_string))
                .unwrap_or_else(|| format!("HTTP {status}")),
        )),
        st => Err(failed(format!("HTTP {st}"))),
    }
}

//! The open-source course server (`mmk server`) from the browser: its HTTP API answers pages of
//! any origin (CORS `*`, also for a server on this machine or the local network), so the page
//! fetches it (`mmkNet.request`). The client is the native hosts' (`mmk_online::Client`), whose
//! requests go through a [`Transport`] that answers from what the page fetched: a call runs, the
//! requests it lacked are fetched, and it runs again until it has every answer (as
//! [`crate::web_tgr`] does for TGRCode). A page on `https://` reaches only `https://` servers
//! and `http://localhost` / `127.0.0.1` (browsers block other plain-HTTP requests from it).
//!
//! The run-again loop is platform-neutral (its fetch is a parameter), so it is tested natively
//! against the server's own handler.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use mmk_online::{Client, Request, Transport};

/// An answer: the status and body, or why there was none.
pub type Answer = Result<(u16, Vec<u8>), String>;

/// Answers fetched for one call, and the requests it made that weren't answered yet.
#[derive(Default)]
struct Exchange {
    got: HashMap<String, Answer>,
    missing: Vec<(String, Request)>,
}

/// A request's identity: method, path, headers and body (the same request is answered alike).
fn key(r: &Request) -> String {
    let body = r.body.as_deref().map(crate::front_course::fnv1a64);
    format!("{} {} {:?} {body:?}", r.method, r.path, r.headers)
}

/// The client's [`Transport`]: what the page fetched, else "on its way" (the request noted).
struct Page(Arc<Mutex<Exchange>>);

impl Transport for Page {
    fn send(&self, _base: &str, req: &Request) -> mmk_online::Result<(u16, Vec<u8>)> {
        let mut x = self.0.lock().map_err(|_| "poisoned".to_string())?;
        let k = key(req);
        if let Some(a) = x.got.get(&k) {
            return a.clone();
        }
        if !x.missing.iter().any(|(m, _)| *m == k) {
            x.missing.push((k, req.clone()));
        }
        Err(format!("{}: on its way", req.path))
    }
}

/// A call runs this many times at most (every call of `mmk_online::Client` makes one request).
const MAX_ROUNDS: usize = 8;

/// Run `f` on a client of the server at `base` (signed in with `token`), its requests answered
/// by `fetch` (base URL, request), until it has every answer.
pub async fn run_with<R, Fut>(
    base: &str,
    token: Option<String>,
    f: impl Fn(&Client) -> R,
    fetch: impl Fn(String, Request) -> Fut,
) -> R
where
    Fut: Future<Output = Answer>,
{
    let exchange = Arc::<Mutex<Exchange>>::default();
    let client = Client::with_transport(base, Arc::new(Page(exchange.clone())))
        .with_token(token.filter(|t| !t.is_empty()));
    let mut rounds = 0;
    loop {
        let r = f(&client);
        let missing = match exchange.lock() {
            Ok(mut x) => std::mem::take(&mut x.missing),
            Err(_) => Vec::new(),
        };
        rounds += 1;
        if missing.is_empty() || rounds > MAX_ROUNDS {
            return r;
        }
        for (k, req) in missing {
            let a = fetch(client.base.clone(), req).await;
            if let Ok(mut x) = exchange.lock() {
                x.got.insert(k, a);
            }
        }
    }
}

/// Course World's source (`tgrcode`, `server`, `library`) from the Mods page's setting and the
/// page's server URL, as natively (`mmk_online::world::Backend::from_setting`): no setting means
/// the server when there is one, else TGRCode; the server setting without a URL is the library.
pub fn world_source(setting: Option<&str>, server: Option<&str>) -> &'static str {
    let server = server.filter(|s| !s.trim().is_empty());
    match (setting.filter(|s| !s.is_empty()), server) {
        (Some("server") | None, Some(_)) => "server",
        (Some("server"), None) | (Some("library"), _) => "library",
        _ => "tgrcode",
    }
}

/// A course server, by base URL (clone it into tasks).
#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub base: String,
}

impl Server {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim().trim_end_matches('/').to_string(),
        }
    }

    /// Run `f` on a client of the server (signed in with `token`), the page fetching what it
    /// asks for, until it has every answer.
    #[cfg(target_arch = "wasm32")]
    pub async fn run<R>(&self, token: Option<String>, f: impl Fn(&Client) -> R) -> R {
        run_with(&self.base, token, f, page::fetch).await
    }
}

#[cfg(target_arch = "wasm32")]
mod page {
    use mmk_online::Request;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    use super::Answer;

    #[wasm_bindgen]
    extern "C" {
        /// `mmkNet.request(method, url, headers, body)`: a promise of `{status, bytes}` (rejects
        /// on a network error); `headers` are `[name, value]` pairs, `body` bytes or null.
        #[wasm_bindgen(catch, js_namespace = mmkNet, js_name = request)]
        fn net_request(
            method: &str,
            url: &str,
            headers: js_sys::Array,
            body: JsValue,
        ) -> Result<js_sys::Promise, JsValue>;
    }

    /// One request through the page, its answer as the native transport's.
    pub async fn fetch(base: String, req: Request) -> Answer {
        let url = format!("{base}{}", req.path);
        let headers = js_sys::Array::new();
        for (k, v) in &req.headers {
            let pair = js_sys::Array::new();
            pair.push(&JsValue::from_str(k));
            pair.push(&JsValue::from_str(v));
            headers.push(&pair);
        }
        let body = match &req.body {
            Some(b) => js_sys::Uint8Array::from(b.as_slice()).into(),
            None => JsValue::NULL,
        };
        let unreachable = || {
            format!(
                "the course server {base} can't be reached (from an https:// page only https:// \
                 servers and http://localhost are)"
            )
        };
        let promise = net_request(req.method, &url, headers, body).map_err(|_| unreachable())?;
        let answer = JsFuture::from(promise).await.map_err(|_| unreachable())?;
        let status = js_sys::Reflect::get(&answer, &"status".into())
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u16;
        let bytes = js_sys::Reflect::get(&answer, &"bytes".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok())
            .map(|a| a.to_vec())
            .unwrap_or_default();
        if bytes.len() as u64 > req.limit {
            return Err(format!("{}: the answer is too large", req.path));
        }
        Ok((status, bytes))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use serde_json::json;

    /// The page's fetch, played by the server's own handler.
    fn handler(
        store: Arc<mmk_server::Store>,
    ) -> impl Fn(String, Request) -> std::future::Ready<Answer> {
        move |_base, r| {
            let headers: Vec<(String, String)> = r
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            let body = r.body.as_deref().unwrap_or_default();
            let (status, _, out) = mmk_server::handle(&store, r.method, &r.path, &headers, body);
            std::future::ready(Ok((status, out)))
        }
    }

    #[test]
    fn the_setting_and_the_url_pick_course_worlds_source() {
        assert_eq!(world_source(None, None), "tgrcode");
        assert_eq!(world_source(None, Some("http://127.0.0.1:8640")), "server");
        assert_eq!(world_source(Some("tgrcode"), Some("http://x")), "tgrcode");
        assert_eq!(world_source(Some("server"), Some(" ")), "library");
        assert_eq!(world_source(Some("library"), Some("http://x")), "library");
        assert_eq!(world_source(Some("other"), None), "tgrcode");
    }

    #[test]
    fn calls_run_again_until_the_page_fetched_their_answers() {
        let dir = std::env::temp_dir().join(format!("mmk-web-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Arc::new(mmk_server::Store::open(&dir).unwrap());
        let fetches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = {
            let (inner, n) = (handler(store.clone()), fetches.clone());
            move |b: String, r: Request| {
                n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                inner(b, r)
            }
        };
        let server = Server::new(" http://127.0.0.1:8640/ ");
        assert_eq!(server.base, "http://127.0.0.1:8640");
        let run = |token: Option<String>, action: &str, args: serde_json::Value| {
            pollster::block_on(run_with(
                &server.base,
                token,
                |c| mmk_online::call_with(c, action, &args),
                &counted,
            ))
        };
        // the Coursebot's upload, under a name no header carries
        let mut course = mmk_formats::smm2::Course::blank("M1");
        course.header.name.set("Made in a browser");
        let data = course.to_bytes().unwrap();
        let args = json!({ "course": mmk_formats::hex_rle::hex_rle(&data), "maker": "ルイージ" });
        let up = run(None, "upload", args).unwrap();
        assert_eq!(up["maker"], "ルイージ");
        assert_eq!(fetches.load(std::sync::atomic::Ordering::SeqCst), 1);
        let code = up["code"].as_str().unwrap().to_string();
        // listing, search, download
        let list = run(None, "search", json!({ "sort": "popular" })).unwrap();
        assert_eq!(list[0]["code"], code.as_str());
        let down = pollster::block_on(run_with(
            &server.base,
            None,
            |c| c.download(&code),
            &counted,
        ));
        assert_eq!(down.unwrap(), data);
        // register, then signed in with the token the page keeps
        let reg = run(None, "register", json!({ "name": "Browser" })).unwrap();
        let token = reg["token"].as_str().unwrap().to_string();
        assert_eq!(
            run(Some(token.clone()), "me", json!({})).unwrap()["name"],
            "Browser"
        );
        assert!(
            run(Some(String::new()), "me", json!({})).is_err(),
            "an empty token is none"
        );
        let mine = run(Some(token), "vote", json!({ "code": code, "like": true })).unwrap();
        assert_eq!(mine["likes"], 1);
        // a refusal in the server's words; a server that can't be reached
        let e = run(None, "info", json!({ "code": "AAA-AAA-AAA" })).unwrap_err();
        assert!(e.contains("HTTP 4"), "{e}");
        let gone = pollster::block_on(run_with(
            &server.base,
            None,
            |c| c.info(&code),
            |_, _| std::future::ready(Err("can't be reached".to_string())),
        ));
        assert_eq!(gone.unwrap_err(), "can't be reached");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

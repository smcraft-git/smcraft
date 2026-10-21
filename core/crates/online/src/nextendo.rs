//! Signing in to Nextendo Network (G6), for Network Play.
//!
//! OAuth 2.0 authorization code with PKCE (S256), as Nextendo's developer docs describe it for
//! third-party apps: the player signs in in their own browser, which comes back to a listener
//! on 127.0.0.1 (port [`CALLBACK_PORT`], `/callback`: the redirect registered with the app,
//! matched exactly by Nextendo); the code is exchanged for an access token
//! (`POST /oauth/token`, then `/api/oauth/token` if that is not there), and the access token
//! for the game token (`GET /api/nex-token`: `nex_token`, `pid`, `username`, `friend_code`;
//! 24 h). The game token goes into the NEX login (`mmk-nex`).
//!
//! The player's password never passes through SMCraft. Tokens are kept only in the host's
//! [`SecretStore`] (the Keychain on Apple; on the desktop shell a private file); nothing here
//! logs a token or the PID.
//!
//! The client ID is the app registered with Nextendo: by default the one the owner registered
//! for the recomp ("SMM2Recomp"); SMCraft may need a registration of its own (its own name and
//! redirect rules), which only the owner can ask for. `SMCRAFT_NEXTENDO_CLIENT_ID` overrides it.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use mmk_nex::crypto::{b64url, random_bytes, sha256};
use serde_json::Value;

use crate::Result;

pub const DEFAULT_API: &str = "https://nextendo.network";
/// The "SMM2Recomp" application (a public client: PKCE, no secret).
pub const DEFAULT_CLIENT_ID: &str = "nxc_R-JWtJ6JrrgY";
pub const SCOPES: &str = "identity game.matchmaking";
/// The loopback port of the registered redirect `http://127.0.0.1:47614/callback`.
pub const CALLBACK_PORT: u16 = 47614;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The account server ("https://nextendo.network").
    pub api: String,
    pub client_id: String,
    pub timeout: Duration,
    /// The sign-in listener's port ([`CALLBACK_PORT`]; 0: any free port, for tests).
    pub callback_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api: DEFAULT_API.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            timeout: Duration::from_secs(10),
            callback_port: CALLBACK_PORT,
        }
    }
}

impl Config {
    /// The defaults, with `SMCRAFT_NEXTENDO_CLIENT_ID` and `SMCRAFT_NEXTENDO_API` (a test
    /// server: only http(s) on a loopback address is taken; anything else is ignored).
    pub fn from_env() -> Self {
        let mut c = Self::default();
        if let Ok(id) = std::env::var("SMCRAFT_NEXTENDO_CLIENT_ID")
            && !id.is_empty()
        {
            c.client_id = id;
        }
        if let Ok(api) = std::env::var("SMCRAFT_NEXTENDO_API")
            && is_loopback_url(&api)
        {
            c.api = api.trim_end_matches('/').into();
        }
        c
    }
}

/// `http(s)://127.0.0.1…` / `localhost` (and nothing that merely starts like it).
pub fn is_loopback_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(h, _)| h);
    host == "localhost" || host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

/// Percent-encoding for URL queries and forms (RFC 3986 unreserved characters kept).
pub fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn unpercent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A query parameter of a URL or request target.
pub fn query_param(target: &str, name: &str) -> Option<String> {
    let q = target.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (unpercent(k) == name).then(|| unpercent(v))
    })
}

/// The PKCE pair: a random verifier (43 characters) and its S256 challenge.
#[derive(Clone, Debug)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        Self::from_verifier(b64url(&random_bytes(32)))
    }

    pub fn from_verifier(verifier: String) -> Self {
        let challenge = b64url(&sha256(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// What the token endpoint gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access_token: String,
    /// Nextendo rotates it on every refresh (the old one stops working soon after).
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

/// What `GET /api/nex-token` gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameToken {
    /// The `nx2.` token for the NEX login (secret).
    pub nex_token: String,
    /// The account's PID: the NEX user name (a credential too: never shown or logged).
    pub pid: u64,
    pub username: String,
    pub friend_code: String,
    pub expires_in: u64,
}

fn agent(cfg: &Config) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(cfg.timeout))
        .build()
        .into()
}

fn read_json(mut r: ureq::http::Response<ureq::Body>) -> Result<(u16, Value)> {
    let status = r.status().as_u16();
    let s = r
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok((status, serde_json::from_str(&s).unwrap_or(Value::Null)))
}

fn token_call(cfg: &Config, form: &str) -> Result<Tokens> {
    let a = agent(cfg);
    let mut last = String::new();
    for path in ["/oauth/token", "/api/oauth/token"] {
        let r = a
            .post(&format!("{}{path}", cfg.api))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .send(form.as_bytes())
            .map_err(|e| format!("Nextendo: {e}"))?;
        let (status, v) = read_json(r)?;
        if status == 404 {
            continue;
        }
        if status != 200 {
            last = v["error"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| format!("HTTP {status}"));
            break;
        }
        let access = v["access_token"].as_str().unwrap_or_default();
        if access.is_empty() {
            return Err("Nextendo gave no access token".into());
        }
        return Ok(Tokens {
            access_token: access.into(),
            refresh_token: v["refresh_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from),
            expires_in: v["expires_in"].as_u64().unwrap_or(3600),
        });
    }
    Err(if last.is_empty() {
        "Nextendo has no token endpoint".into()
    } else {
        format!("Nextendo refused the sign-in ({last})")
    })
}

/// Exchanges an authorization code.
pub fn exchange_code(
    cfg: &Config,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<Tokens> {
    token_call(
        cfg,
        &format!(
            "grant_type=authorization_code&client_id={}&code={}&redirect_uri={}&code_verifier={}",
            percent(&cfg.client_id),
            percent(code),
            percent(redirect_uri),
            percent(verifier)
        ),
    )
}

/// A new access token (and the rotated refresh token).
pub fn refresh(cfg: &Config, refresh_token: &str) -> Result<Tokens> {
    token_call(
        cfg,
        &format!(
            "grant_type=refresh_token&refresh_token={}&client_id={}",
            percent(refresh_token),
            percent(&cfg.client_id)
        ),
    )
}

/// The game token for an access token.
pub fn game_token(cfg: &Config, access_token: &str) -> Result<GameToken> {
    let r = agent(cfg)
        .get(&format!("{}/api/nex-token", cfg.api))
        .header("Authorization", &format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .call()
        .map_err(|e| format!("Nextendo: {e}"))?;
    let (status, v) = read_json(r)?;
    if status != 200 {
        return Err(match v["error"].as_str() {
            Some(e) => format!("Nextendo: {e}"),
            None => format!("Nextendo answered HTTP {status} for the game token"),
        });
    }
    let nex = v["nex_token"].as_str().unwrap_or_default();
    let pid = v["pid"].as_u64().unwrap_or(0);
    if nex.is_empty() || pid == 0 {
        return Err("Nextendo gave no game token".into());
    }
    Ok(GameToken {
        nex_token: nex.into(),
        pid,
        username: v["username"].as_str().unwrap_or_default().into(),
        friend_code: v["friend_code"].as_str().unwrap_or_default().into(),
        expires_in: v["expires_in"].as_u64().unwrap_or(86_400),
    })
}

/// A sign-in waiting for the browser: open [`SignIn::url`], then [`SignIn::finish`].
pub struct SignIn {
    listener: TcpListener,
    pub url: String,
    pub redirect_uri: String,
    state: String,
    pkce: Pkce,
    cfg: Config,
}

impl SignIn {
    /// Opens the loopback listener (127.0.0.1 only) and builds the authorize URL.
    pub fn begin(cfg: &Config) -> Result<Self> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, cfg.callback_port)).map_err(|e| {
                format!(
                    "the sign-in needs port {} on this computer for Nextendo's reply: {e}",
                    cfg.callback_port
                )
            })?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let state = b64url(&random_bytes(16));
        let pkce = Pkce::new();
        let url = format!(
            "{}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}\
             &code_challenge={}&code_challenge_method=S256",
            cfg.api,
            percent(&cfg.client_id),
            percent(&redirect_uri),
            percent(SCOPES),
            state,
            pkce.challenge
        );
        Ok(Self {
            listener,
            url,
            redirect_uri,
            state,
            pkce,
            cfg: cfg.clone(),
        })
    }

    /// Waits (at most `timeout`) for the browser to come back, answers it with a short page,
    /// and exchanges the code. A wrong `state` is refused (a forged callback); `error=…`
    /// (the player said no) ends the sign-in.
    pub fn finish(self, timeout: Duration) -> Result<Tokens> {
        self.listener
            .set_nonblocking(true)
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Err("the sign-in timed out (the browser did not come back)".into());
            }
            let (stream, from) = match self.listener.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            if !from.ip().is_loopback() {
                continue;
            }
            match self.callback(stream, from) {
                Some(Ok(code)) => {
                    return exchange_code(
                        &self.cfg,
                        &code,
                        &self.redirect_uri,
                        &self.pkce.verifier,
                    );
                }
                Some(Err(e)) => return Err(e),
                None => {} // not the callback (a favicon, a probe): keep waiting
            }
        }
    }

    /// One request on the listener: Some(code) for a good callback.
    fn callback(&self, stream: TcpStream, _from: SocketAddr) -> Option<Result<String>> {
        stream.set_nonblocking(false).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let target = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string();
        let reply = |mut s: &TcpStream, status: &str, text: &str| {
            let body = format!(
                "<!doctype html><meta charset=utf-8><title>SMCraft</title>\
                 <body style=\"font-family:sans-serif;margin:3em\"><p>{text}</p>"
            );
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
        };
        if !target.starts_with("/callback") {
            reply(&stream, "404 Not Found", "Not here.");
            return None;
        }
        if query_param(&target, "state").as_deref() != Some(self.state.as_str()) {
            reply(
                &stream,
                "400 Bad Request",
                "This sign-in link is not the one SMCraft started.",
            );
            return Some(Err(
                "the sign-in came back with the wrong state (refused)".into()
            ));
        }
        if let Some(e) = query_param(&target, "error") {
            reply(
                &stream,
                "200 OK",
                "Sign-in cancelled. You can close this tab.",
            );
            return Some(Err(if e == "access_denied" {
                "the sign-in was cancelled".into()
            } else {
                format!("Nextendo: {e}")
            }));
        }
        match query_param(&target, "code").filter(|c| !c.is_empty()) {
            Some(code) => {
                reply(
                    &stream,
                    "200 OK",
                    "Signed in to Nextendo. You can close this tab and go back to SMCraft.",
                );
                Some(Ok(code))
            }
            None => {
                reply(&stream, "400 Bad Request", "No sign-in code.");
                Some(Err("the sign-in came back without a code".into()))
            }
        }
    }
}

/// Where the host keeps secrets (the refresh token).
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> std::io::Result<()>;
    fn delete(&self, key: &str);
}

/// A private file (mode 0600 on Unix): the desktop shell's store.
pub struct FileStore {
    pub path: PathBuf,
}

impl FileStore {
    fn load(&self) -> serde_json::Map<String, Value> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, m: &serde_json::Map<String, Value>) -> std::io::Result<()> {
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = self.path.with_extension("tmp");
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(m).unwrap())?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.path)
    }
}

impl SecretStore for FileStore {
    fn get(&self, key: &str) -> Option<String> {
        self.load().get(key)?.as_str().map(String::from)
    }
    fn set(&self, key: &str, value: &str) -> std::io::Result<()> {
        let mut m = self.load();
        m.insert(
            "_note".into(),
            "Private: Nextendo sign-in. Never share this file.".into(),
        );
        m.insert(key.into(), value.into());
        self.save(&m)
    }
    fn delete(&self, key: &str) {
        let mut m = self.load();
        if m.remove(key).is_some() {
            let _ = self.save(&m);
        }
    }
}

const REFRESH_KEY: &str = "nextendo.refresh_token";

/// The Nextendo account as the app uses it.
pub struct Account<'a> {
    pub cfg: Config,
    pub store: &'a dyn SecretStore,
}

impl Account<'_> {
    pub fn signed_in(&self) -> bool {
        self.store.get(REFRESH_KEY).is_some()
    }

    /// Runs a sign-in: `open` shows the URL (the browser); waits up to `timeout`.
    pub fn sign_in(&self, open: impl FnOnce(&str), timeout: Duration) -> Result<GameToken> {
        let s = SignIn::begin(&self.cfg)?;
        open(&s.url);
        let t = s.finish(timeout)?;
        self.keep(&t)?;
        game_token(&self.cfg, &t.access_token)
    }

    fn keep(&self, t: &Tokens) -> Result<()> {
        if let Some(r) = &t.refresh_token {
            // stored before the access token is used: a refresh token is good once
            self.store
                .set(REFRESH_KEY, r)
                .map_err(|e| format!("could not keep the sign-in: {e}"))?;
        }
        Ok(())
    }

    /// A fresh game token from the stored sign-in (refreshing it). A refused refresh token
    /// ends the sign-in.
    pub fn game_token(&self) -> Result<GameToken> {
        let r = self
            .store
            .get(REFRESH_KEY)
            .ok_or("not signed in to Nextendo")?;
        let t = match refresh(&self.cfg, &r) {
            Ok(t) => t,
            Err(e) if e.contains("invalid_grant") => {
                self.store.delete(REFRESH_KEY);
                return Err("the Nextendo sign-in ended; sign in again".into());
            }
            Err(e) => return Err(e),
        };
        self.keep(&t)?;
        game_token(&self.cfg, &t.access_token)
    }

    pub fn sign_out(&self) {
        self.store.delete(REFRESH_KEY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Read;
    use std::sync::{Arc, Mutex};

    #[test]
    fn pkce_and_urls() {
        // RFC 7636 appendix B
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        assert_eq!(Pkce::new().verifier.len(), 43);
        assert_eq!(
            percent("identity game.matchmaking"),
            "identity%20game.matchmaking"
        );
        assert_eq!(
            query_param("/callback?code=a%2Bb&state=x", "code").as_deref(),
            Some("a+b")
        );
        assert!(is_loopback_url("http://127.0.0.1:5000"));
        assert!(is_loopback_url("http://localhost/x"));
        assert!(!is_loopback_url("http://127.0.0.1.example.com/"));
        assert!(!is_loopback_url("https://nextendo.network"));
    }

    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, String>>);

    impl SecretStore for Memory {
        fn get(&self, key: &str) -> Option<String> {
            self.0.lock().unwrap().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) -> std::io::Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, key: &str) {
            self.0.lock().unwrap().remove(key);
        }
    }

    /// A stand-in for Nextendo's account server: /oauth/authorize redirects back with a code,
    /// /oauth/token checks PKCE and rotates refresh tokens, /api/nex-token answers.
    struct Mock {
        base: String,
        refreshes: Arc<Mutex<u32>>,
    }

    fn mock(client_id: &'static str) -> Mock {
        let ln = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", ln.local_addr().unwrap());
        let refreshes = Arc::new(Mutex::new(0u32));
        let count = refreshes.clone();
        std::thread::spawn(move || {
            let mut challenge = String::new();
            let mut refresh = String::from("R0");
            for s in ln.incoming() {
                let Ok(mut s) = s else { continue };
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let target = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut len = 0;
                let mut auth = String::new();
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    let lower = h.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = h["authorization:".len()..].trim().to_string();
                    }
                }
                let mut body = vec![0u8; len];
                r.read_exact(&mut body).unwrap();
                let form = format!("?{}", String::from_utf8_lossy(&body));
                let p = |k: &str| query_param(&form, k).unwrap_or_default();
                let (status, json) = if target.starts_with("/oauth/authorize") {
                    assert_eq!(query_param(&target, "client_id").unwrap(), client_id);
                    assert_eq!(query_param(&target, "scope").unwrap(), SCOPES);
                    challenge = query_param(&target, "code_challenge").unwrap();
                    let back = format!(
                        "{}?code=C1&state={}",
                        query_param(&target, "redirect_uri").unwrap(),
                        query_param(&target, "state").unwrap()
                    );
                    let _ = write!(
                        s,
                        "HTTP/1.1 302 Found\r\nLocation: {back}\r\nContent-Length: 0\r\n\r\n"
                    );
                    continue;
                } else if target == "/oauth/token" {
                    (404, "{}".to_string())
                } else if target == "/api/oauth/token" && p("grant_type") == "authorization_code" {
                    let ok = p("code") == "C1"
                        && p("client_id") == client_id
                        && b64url(&sha256(p("code_verifier").as_bytes())) == challenge;
                    if ok {
                        (
                            200,
                            r#"{"access_token":"A1","refresh_token":"R1","expires_in":3600}"#
                                .into(),
                        )
                    } else {
                        (400, r#"{"error":"invalid_grant"}"#.into())
                    }
                } else if target == "/api/oauth/token" && p("grant_type") == "refresh_token" {
                    if p("refresh_token") == refresh
                        || refresh == "R0" && p("refresh_token") == "R1"
                    {
                        let mut n = count.lock().unwrap();
                        *n += 1;
                        refresh = format!("R{}", *n + 1);
                        (
                            200,
                            format!(
                                r#"{{"access_token":"A{}","refresh_token":"{refresh}"}}"#,
                                *n + 1
                            ),
                        )
                    } else {
                        (400, r#"{"error":"invalid_grant"}"#.into())
                    }
                } else if target == "/api/nex-token" && auth.starts_with("Bearer A") {
                    (200, r#"{"nex_token":"nx2.test.sig","pid":1234567,"friend_code":"SW-0000-0000-0000","username":"tester","expires_in":86400}"#.into())
                } else {
                    (401, r#"{"error":"unauthorized"}"#.into())
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                    json.len()
                );
            }
        });
        Mock { base, refreshes }
    }

    /// Plays the browser: follows the authorize redirect to the loopback callback.
    fn browser(url: String) {
        std::thread::spawn(move || {
            let a: ureq::Agent = ureq::Agent::config_builder()
                .max_redirects(5)
                .http_status_as_error(false)
                .build()
                .into();
            let _ = a.get(&url).call();
        });
    }

    #[test]
    fn sign_in_refresh_rotation_and_game_token() {
        let m = mock("nxc_test");
        let cfg = Config {
            api: m.base.clone(),
            client_id: "nxc_test".into(),
            timeout: Duration::from_secs(5),
            callback_port: 0,
        };
        let store = Memory::default();
        let acct = Account {
            cfg: cfg.clone(),
            store: &store,
        };
        assert!(!acct.signed_in());
        let g = acct
            .sign_in(|url| browser(url.to_string()), Duration::from_secs(10))
            .unwrap();
        assert_eq!((g.pid, g.username.as_str()), (1234567, "tester"));
        assert_eq!(g.nex_token, "nx2.test.sig");
        assert_eq!(store.get(REFRESH_KEY).as_deref(), Some("R1"));
        // each refresh stores the rotated token first
        acct.game_token().unwrap();
        assert_eq!(store.get(REFRESH_KEY).as_deref(), Some("R2"));
        acct.game_token().unwrap();
        assert_eq!(store.get(REFRESH_KEY).as_deref(), Some("R3"));
        assert_eq!(*m.refreshes.lock().unwrap(), 2);
        // a refresh token the server no longer takes ends the sign-in
        store.set(REFRESH_KEY, "R1").unwrap();
        assert!(acct.game_token().unwrap_err().contains("sign in again"));
        assert!(!acct.signed_in());
    }

    #[test]
    fn forged_or_cancelled_callbacks_are_refused() {
        let cfg = Config {
            api: "http://127.0.0.1:9".into(),
            callback_port: 0,
            ..Default::default()
        };
        for (query, want) in [
            ("code=C1&state=forged", "wrong state"),
            ("error=access_denied&state={state}", "cancelled"),
        ] {
            let s = SignIn::begin(&cfg).unwrap();
            assert!(s.redirect_uri.starts_with("http://127.0.0.1:"));
            assert!(s.url.contains("code_challenge_method=S256"));
            let state = query_param(&s.url, "state").unwrap();
            let back = format!("{}?{}", s.redirect_uri, query.replace("{state}", &state));
            browser(back);
            let e = s.finish(Duration::from_secs(5)).unwrap_err();
            assert!(e.contains(want), "{e}");
        }
        let s = SignIn::begin(&cfg).unwrap();
        assert!(
            s.finish(Duration::from_millis(200))
                .unwrap_err()
                .contains("timed out")
        );
    }

    #[test]
    fn file_store_is_private() {
        let dir = std::env::temp_dir().join(format!("smcraft-store-{}", std::process::id()));
        let f = FileStore {
            path: dir.join("nextendo.json"),
        };
        f.set("k", "v").unwrap();
        assert_eq!(f.get("k").as_deref(), Some("v"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&f.path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        f.delete("k");
        assert_eq!(f.get("k"), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}

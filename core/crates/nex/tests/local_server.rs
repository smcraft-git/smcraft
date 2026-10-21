//! Against a local nextendo-nex (scripts/nex-local.sh builds it into core/target/nex-local/;
//! or name the binary with SMCRAFT_NEX_LOCAL). Without it every test here says so and passes,
//! so CI (no Go, no clone of the server) skips them.
//!
//! Login with a game token in Nextendo's shape → the secure connection → Register → two
//! clients matched into one SMCraft session → they punch through to each other on loopback →
//! `mmk-netplay` sessions run over the punched sockets and agree.

use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mmk_nex::client::{self, GameServer, Secure};
use mmk_nex::matchmaking::*;
use mmk_nex::play::{self, PlayConfig};
use mmk_nex::{Error, StationUrl, token};

const ACCESS_KEY: &str = "0f1e2d3c";

struct LocalServer {
    child: Child,
    auth: String,
    /// With TLS: the certificate's key pin.
    pin: Option<String>,
    secret: Vec<u8>,
    log: Arc<Mutex<Vec<String>>>,
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        // SMCRAFT_NEX_LOG=1 prints what the server logged (cargo test -- --nocapture)
        if std::env::var_os("SMCRAFT_NEX_LOG").is_some() {
            for l in self.log.lock().unwrap().iter() {
                eprintln!("  server: {l}");
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl LocalServer {
    fn start() -> Option<Self> {
        Self::start_with(&[])
    }

    fn start_with(extra: &[&str]) -> Option<Self> {
        let bin = std::env::var_os("SMCRAFT_NEX_LOCAL")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/nex-local/nex-local")
            });
        if !bin.exists() {
            eprintln!("skipped: no local NEX server (scripts/nex-local.sh builds one)");
            return None;
        }
        let secret = mmk_nex::crypto::random_bytes(16);
        let mut child = Command::new(&bin)
            .args(["-access-key", ACCESS_KEY, "-token-secret"])
            .arg(mmk_nex::crypto::hex(&secret))
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the local NEX server");
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        out.read_line(&mut first).unwrap();
        let auth = first
            .split_whitespace()
            .find_map(|w| w.strip_prefix("auth="))
            .expect("READY line")
            .to_string();
        let pin = first
            .split_whitespace()
            .find_map(|w| w.strip_prefix("pin="))
            .map(String::from);
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = log.clone();
        std::thread::spawn(move || {
            for line in out.lines().map_while(Result::ok) {
                sink.lock().unwrap().push(line);
            }
        });
        let scheme = if pin.is_some() { "wss" } else { "ws" };
        Some(Self {
            child,
            auth: format!("{scheme}://{auth}/"),
            pin,
            // the server takes the secret as the text given on its command line
            secret: mmk_nex::crypto::hex(&secret).into_bytes(),
            log,
        })
    }

    fn game_server(&self) -> GameServer {
        let mut g = GameServer::smm2(&self.auth, ACCESS_KEY);
        g.timeout = Duration::from_secs(5);
        if let Some(pin) = &self.pin {
            let pin = mmk_nex::tls::parse_pin(pin).unwrap();
            g.tls = Some(mmk_nex::tls::pinned("nex-local.test", pin).unwrap());
        }
        g
    }

    fn id_token(&self, pid: u64) -> String {
        let nx2 = token::nx2_token_for_a_day(&self.secret, pid, "tester");
        token::id_token(pid, &nx2, b"local test key")
    }

    fn logged(&self, what: &str) -> bool {
        self.log.lock().unwrap().iter().any(|l| l.contains(what))
    }
}

fn secure(s: &LocalServer, pid: u64) -> Secure {
    let login = client::login(&s.game_server(), pid, &s.id_token(pid)).expect("login");
    assert_eq!(login.pid, pid);
    assert_eq!(login.server_name, "nex-local");
    assert!(login.secure_station.socket_addr().is_some());
    Secure::connect(&s.game_server(), &login).expect("secure connection")
}

fn local_url(port: u16) -> StationUrl {
    StationUrl::parse(&format!(
        "prudp:/address=127.0.0.1;port={port};Pl=2;natf=0;natm=0;pmp=0;sid=15;upnp=0"
    ))
}

#[test]
fn login_secure_connection_and_register() {
    let Some(s) = LocalServer::start() else {
        return;
    };
    let mut c = secure(&s, 1_000_001);
    let public = c.register(&[local_url(7000)]).expect("register");
    assert!(c.rvcid > 0);
    assert_eq!(public.get("RVCID"), Some(c.rvcid.to_string().as_str()));
    assert_eq!(public.get_u64("PID"), Some(1_000_001));
    // the connection stays usable across pings
    c.poll(Duration::from_millis(300)).unwrap();
    let urls = c.session_urls(12345).unwrap();
    assert!(urls.is_empty(), "no such gathering");
    c.disconnect();
    assert!(s.logged("login accepted"));
}

#[test]
fn a_bad_game_token_or_access_key_is_refused() {
    let Some(s) = LocalServer::start() else {
        return;
    };
    let pid = 1_000_002;
    let forged = token::id_token(pid, &token::nx2_token_for_a_day(b"wrong", pid, "x"), b"k");
    match client::login(&s.game_server(), pid, &forged) {
        Err(Error::Rmc { code, .. }) => assert_eq!(code & 0x7FFF_FFFF, 0x0068_0002),
        other => panic!("forged token: {:?}", other.map(|l| l.pid)),
    }
    // a token for someone else
    let theirs = s.id_token(pid + 1);
    assert!(client::login(&s.game_server(), pid, &theirs).is_err());
    let mut g = s.game_server();
    g.access_key = "not-it00".into();
    g.timeout = Duration::from_secs(2);
    assert!(matches!(
        client::login(&g, pid, &s.id_token(pid)),
        Err(Error::Timeout(_))
    ));
    assert!(s.logged("bad nx2 signature"));
}

#[test]
fn smcraft_sessions_never_meet_console_ones() {
    let Some(s) = LocalServer::start() else {
        return;
    };
    let mut console = secure(&s, 1_000_010);
    console.register(&[local_url(7001)]).unwrap();
    let mut coop = MatchmakeSession::smcraft(smcraft::KIND_COOP, 4, 1);
    coop.game_mode = 11; // what SMM2 uses for co-op
    coop.attribs = vec![0, 0, 0, 0, 1_742_398_235, 0];
    coop.application_data = vec![1, 2, 3];
    let theirs = console
        .auto_matchmake(&AutoMatchmakeParam {
            session: coop,
            participation_count: 1,
            ..Default::default()
        })
        .unwrap();
    let mut me = secure(&s, 1_000_011);
    me.register(&[local_url(7002)]).unwrap();
    let ours = me
        .auto_matchmake(&AutoMatchmakeParam {
            session: MatchmakeSession::smcraft(smcraft::KIND_COOP, 2, 1),
            criteria: vec![SearchCriteria::smcraft(smcraft::KIND_COOP, 1)],
            participation_count: 1,
            ..Default::default()
        })
        .unwrap();
    assert_ne!(ours.id, theirs.id);
    assert_eq!(ours.owner, 1_000_011, "a session of our own");
    assert!(ours.is_smcraft());
    assert_eq!(ours.participants, 1);
    // browsing shows both, and only one is SMCraft's
    let all = me
        .browse(&SearchCriteria::smcraft(smcraft::KIND_COOP, 1))
        .unwrap();
    assert_eq!(all.iter().filter(|x| x.is_smcraft()).count(), 1);
    // a second SMCraft player is put in ours
    let mut other = secure(&s, 1_000_012);
    other.register(&[local_url(7003)]).unwrap();
    let joined = other
        .auto_matchmake(&AutoMatchmakeParam {
            session: MatchmakeSession::smcraft(smcraft::KIND_COOP, 2, 1),
            participation_count: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(joined.id, ours.id);
    assert_eq!(joined.participants, 2);
    let who: Vec<u64> = other
        .participants(ours.id)
        .unwrap()
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(who, [1_000_011, 1_000_012]);
    // and the owner hears about it
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut heard = false;
    while !heard && Instant::now() < deadline {
        heard = me
            .poll(Duration::from_millis(100))
            .unwrap()
            .iter()
            .any(|e| {
                matches!(e, client::Event::Notification(n)
                if n.kind == NOTIFY_PARTICIPATE && n.param2 == 1_000_012)
            });
    }
    assert!(heard, "Participate notification");
}

fn config(s: &LocalServer, pid: u64) -> PlayConfig {
    PlayConfig {
        server: s.game_server(),
        pid,
        id_token: s.id_token(pid),
        kind: smcraft::KIND_COOP,
        players: 2,
        course: 0xC0FFEE,
        bind: "127.0.0.1:0".parse().unwrap(),
        advertise: Some(Ipv4Addr::LOCALHOST),
        nat_check: None,
        wait: Duration::from_secs(15),
        log: Arc::new(|_| {}),
    }
}

#[test]
fn network_play_end_to_end() {
    let Some(s) = LocalServer::start() else {
        return;
    };
    let s = Arc::new(s);
    let s2 = s.clone();
    let host = std::thread::spawn(move || play::find_match(config(&s2, 3_000_001)));
    std::thread::sleep(Duration::from_millis(700));
    let joiner = play::find_match(config(&s, 3_000_002)).expect("joiner");
    let host = host.join().unwrap().expect("host");
    assert!(host.host && !joiner.host);
    assert_eq!(host.gid, joiner.gid);
    assert_eq!((host.me, joiner.me), (0, 1));
    assert_eq!(host.peers, [joiner.socket.local_addr().unwrap()]);
    assert_eq!(joiner.peers, [host.socket.local_addr().unwrap()]);

    // the rollback sessions over the punched sockets
    use mmk_netplay::{Link, Session, UdpLink};
    use mmk_sim::{Input, World, input, level::Level};
    let mut world = World::with_level(Level::demo());
    world.set_players(2);
    let mut a = Session::new(world.clone(), 2, host.me);
    let mut b = Session::new(world, 2, joiner.me);
    let mut la =
        UdpLink::from_socket(host.socket.try_clone().unwrap(), host.peers.clone()).unwrap();
    let mut lb =
        UdpLink::from_socket(joiner.socket.try_clone().unwrap(), joiner.peers.clone()).unwrap();
    let script = |p: usize, f: usize| Input {
        buttons: match (p, f / 30 % 3) {
            (0, 0) | (1, 1) => input::RIGHT,
            (0, 1) => input::RIGHT | input::JUMP,
            _ => input::RUN | input::RIGHT,
        },
    };
    let end = Instant::now() + Duration::from_secs(20);
    let frames = 240;
    while (a.frame < frames + 20 || b.frame < frames + 20) && Instant::now() < end {
        for p in la.poll() {
            a.receive(&p);
        }
        for p in lb.poll() {
            b.receive(&p);
        }
        if a.can_advance() {
            let f = a.frame;
            a.advance(if f < frames {
                script(0, f)
            } else {
                Input::default()
            });
        }
        if b.can_advance() {
            let f = b.frame;
            b.advance(if f < frames {
                script(1, f)
            } else {
                Input::default()
            });
        }
        la.send(&a.packet());
        lb.send(&b.packet());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(a.frame >= frames && b.frame >= frames, "both sessions ran");
    assert_eq!((a.desync, b.desync), (None, None));
    assert!(a.checks_compared > 0, "the peers compared state hashes");
    let _: SocketAddr = host.peers[0];
}

#[test]
fn over_tls_with_the_key_pinned() {
    let Some(s) = LocalServer::start_with(&["-tls"]) else {
        return;
    };
    assert!(s.auth.starts_with("wss://"));
    let mut c = secure(&s, 1_000_020);
    c.register(&[local_url(7004)]).expect("register over TLS");
    c.disconnect();
    // another key: refused before anything is sent
    let mut g = s.game_server();
    g.tls = Some(mmk_nex::tls::pinned("nex-local.test", [7; 32]).unwrap());
    match client::login(&g, 1_000_020, &s.id_token(1_000_020)) {
        Err(Error::Protocol(e)) => assert!(e.contains("TLS"), "{e}"),
        other => panic!("a wrong pin: {:?}", other.map(|l| l.pid)),
    }
    std::thread::sleep(Duration::from_millis(200));
    let logins = s
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains("login accepted"))
        .count();
    assert_eq!(logins, 1, "the second login never reached the server");
}

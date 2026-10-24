//! Network Play for the front end (`mmk_front::netplay::Network`): the account, the search on a
//! thread (`mmk_nex::play::find_match`, cancellable, with its progress), and the match's link
//! for `mmk-netplay` — a UDP link over the punched socket that also notices a peer leaving
//! (its goodbye datagram, or silence).
//!
//! Only a local NEX test server (`scripts/nex-local.sh`, `mmk-play --nextendo-local`) is
//! supported: its test accounts carry game tokens made with the server's secret. Nextendo's live
//! servers wait for Nextendo's agreement and SMCraft's registration (PROGRESS.md G6); nothing
//! here connects anywhere by itself.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mmk_front::netplay::{LinkState, MatchInfo, MatchRequest, MatchState, NetAccount};
use mmk_netplay::{Link, Packet};
use mmk_nex::matchmaking::smcraft;
use mmk_nex::play::{self, Match, PlayConfig};
use mmk_nex::{crypto, token};

/// A local NEX test server: its authentication address, PRUDP access key and token secret.
#[derive(Clone, Debug)]
pub struct LocalServer {
    pub addr: String,
    pub access_key: String,
    pub token_secret: String,
}

/// The goodbye a player sends when it leaves a match (not a `mmk-netplay` packet).
const BYE: &[u8] = b"SMCBYE";
/// Datagrams of the hole punch, which can still arrive once the match runs.
const PUNCH: &[u8] = b"SMCP";
/// A peer silent this long has gone; before its first packet (the countdown and the loading
/// run first) the wait is longer.
const SILENCE: Duration = Duration::from_secs(5);
const FIRST_PACKET: Duration = Duration::from_secs(15);

/// What the link saw of the other players.
#[derive(Debug)]
struct Heard {
    since: Instant,
    last: Option<Instant>,
    bye: bool,
}

/// `mmk-netplay`'s link over the match's socket.
pub struct NetLink {
    socket: UdpSocket,
    peers: Vec<SocketAddr>,
    heard: Arc<Mutex<Heard>>,
}

impl Link for NetLink {
    fn send(&mut self, p: &Packet) {
        let b = p.to_bytes();
        for peer in &self.peers {
            let _ = self.socket.send_to(&b, peer);
        }
    }

    fn poll(&mut self) -> Vec<Packet> {
        let mut out = Vec::new();
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            if !self.peers.contains(&from) {
                continue;
            }
            let d = &buf[..n];
            if d.starts_with(BYE) {
                self.heard.lock().unwrap().bye = true;
            } else if d.starts_with(PUNCH) {
                continue;
            } else if let Some(p) = Packet::parse(d) {
                self.heard.lock().unwrap().last = Some(Instant::now());
                out.push(p);
            }
        }
        out
    }
}

struct Search {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<(usize, usize)>>,
    rx: Receiver<Result<Match, String>>,
}

/// The play config for a test account of a local server.
#[allow(clippy::too_many_arguments)]
pub fn local_config(
    server: &LocalServer,
    pid: u64,
    kind: u32,
    players: u16,
    course: u64,
    bind: SocketAddr,
    log: play::Log,
) -> PlayConfig {
    let nx2 = token::nx2_token_for_a_day(server.token_secret.as_bytes(), pid, "player");
    PlayConfig {
        server: mmk_nex::GameServer::smm2(&format!("ws://{}/", server.addr), &server.access_key),
        pid,
        id_token: token::id_token(pid, &nx2, &crypto::random_bytes(32)),
        kind,
        players,
        course,
        pick: Vec::new(),
        bind,
        advertise: None,
        nat_check: None,
        wait: Duration::from_secs(300),
        log,
        cancel: None,
        progress: None,
    }
}

/// A test account's PID (a random one unless given).
pub fn test_pid(fixed: Option<u64>) -> u64 {
    fixed.unwrap_or_else(|| {
        1_000_000
            + u64::from(u32::from_le_bytes(
                crypto::random_bytes(4).try_into().unwrap(),
            )) % 1_000_000_000
    })
}

/// The match being played.
struct Playing {
    /// Its keep-alive: leaves the session when dropped.
    _keepalive: Match,
    /// For the goodbye.
    socket: UdpSocket,
    peers: Vec<SocketAddr>,
    /// What the link heard.
    heard: Arc<Mutex<Heard>>,
}

/// The host's Network Play.
pub struct NetworkHost {
    server: LocalServer,
    fixed_pid: Option<u64>,
    name: String,
    /// The signed-in test account (PID; never shown or logged).
    account: Option<u64>,
    search: Option<Search>,
    failed: Option<String>,
    /// The match found, until it is played (its socket then moves into the link) and left.
    found: Option<(Match, MatchInfo)>,
    playing: Option<Playing>,
    lost_logged: bool,
}

impl NetworkHost {
    /// Network Play over a local test server, signed in as a test account named `name`.
    pub fn local(server: LocalServer, pid: Option<u64>, name: &str) -> Self {
        Self {
            server,
            fixed_pid: pid,
            name: name.to_string(),
            account: Some(test_pid(pid)),
            search: None,
            failed: None,
            found: None,
            playing: None,
            lost_logged: false,
        }
    }

    pub fn account(&self) -> NetAccount {
        match self.account {
            Some(_) => NetAccount::SignedIn(self.name.clone()),
            None => NetAccount::SignedOut,
        }
    }

    pub fn sign_in(&mut self) {
        self.account = Some(test_pid(self.fixed_pid));
    }

    pub fn sign_out(&mut self) {
        self.cancel_match();
        self.leave_match();
        self.account = None;
    }

    pub fn find_match(&mut self, r: &MatchRequest) {
        self.cancel_match();
        self.leave_match();
        self.failed = None;
        let Some(pid) = self.account else {
            self.failed = Some("not signed in".into());
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new((0, r.players)));
        let mut cfg = local_config(
            &self.server,
            pid,
            if r.versus {
                smcraft::KIND_VERSUS
            } else {
                smcraft::KIND_COOP
            },
            r.players as u16,
            r.pool,
            "0.0.0.0:0".parse().unwrap(),
            Arc::new(|m: &str| crate::app::log(m)),
        );
        cfg.pick = r.pick.clone().unwrap_or_default().into_bytes();
        cfg.cancel = Some(cancel.clone());
        let p = progress.clone();
        cfg.progress = Some(Arc::new(move |found, wanted| {
            *p.lock().unwrap() = (found, wanted)
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = play::find_match(cfg).map_err(|e| mmk_nex::client::describe(&e));
            let _ = tx.send(r);
        });
        self.search = Some(Search {
            cancel,
            progress,
            rx,
        });
    }

    pub fn cancel_match(&mut self) {
        if let Some(s) = self.search.take() {
            s.cancel.store(true, Ordering::Relaxed);
            // the thread leaves its session and ends by itself
            crate::app::log("Network Play: search cancelled");
        }
    }

    pub fn poll_match(&mut self) -> MatchState {
        if let Some((_, info)) = &self.found {
            return MatchState::Ready(info.clone());
        }
        if let Some(e) = &self.failed {
            return MatchState::Failed(e.clone());
        }
        let Some(s) = &self.search else {
            return MatchState::Idle;
        };
        match s.rx.try_recv() {
            Ok(Ok(m)) => {
                self.search = None;
                let info = MatchInfo {
                    me: m.me,
                    players: m.players,
                    gid: m.gid,
                    course: String::from_utf8(m.pick.clone())
                        .ok()
                        .filter(|c| !c.is_empty()),
                };
                self.found = Some((m, info.clone()));
                MatchState::Ready(info)
            }
            Ok(Err(e)) => {
                self.search = None;
                self.failed = Some(e.clone());
                MatchState::Failed(e)
            }
            Err(TryRecvError::Empty) => {
                let (found, wanted) = *s.progress.lock().unwrap();
                MatchState::Searching { found, wanted }
            }
            Err(TryRecvError::Disconnected) => {
                self.search = None;
                MatchState::Failed("the search stopped".into())
            }
        }
    }

    /// The found match's link for the rollback session: (link, players, our position).
    pub fn start_link(&mut self) -> Result<(NetLink, usize, usize), String> {
        let (m, info) = self.found.take().ok_or("no match to play")?;
        let socket = m.socket.try_clone().map_err(|e| e.to_string())?;
        let bye = m.socket.try_clone().map_err(|e| e.to_string())?;
        let heard = Arc::new(Mutex::new(Heard {
            since: Instant::now(),
            last: None,
            bye: false,
        }));
        let link = NetLink {
            socket,
            peers: m.peers.clone(),
            heard: heard.clone(),
        };
        let peers = m.peers.clone();
        self.playing = Some(Playing {
            _keepalive: m,
            socket: bye,
            peers,
            heard,
        });
        self.lost_logged = false;
        Ok((link, info.players, info.me))
    }

    pub fn link(&mut self) -> LinkState {
        let Some(p) = &self.playing else {
            return LinkState::Lost;
        };
        let h = p.heard.lock().unwrap();
        let silent = match h.last {
            Some(t) => t.elapsed() > SILENCE,
            None => h.since.elapsed() > FIRST_PACKET,
        };
        if !(h.bye || silent) {
            return LinkState::Up;
        }
        if !self.lost_logged {
            self.lost_logged = true;
            crate::app::log(if h.bye {
                "Network Play: a player left the match"
            } else {
                "Network Play: the other players stopped sending"
            });
        }
        LinkState::Lost
    }

    /// Leave the match: say goodbye to the peers, then drop it (it leaves the session).
    pub fn leave_match(&mut self) {
        self.found = None;
        if let Some(p) = self.playing.take() {
            crate::app::log("Network Play: leaving the match");
            for _ in 0..3 {
                for to in &p.peers {
                    let _ = p.socket.send_to(BYE, to);
                }
            }
            // (dropping the match leaves the session)
        }
    }
}

impl Drop for NetworkHost {
    fn drop(&mut self) {
        self.cancel_match();
        self.leave_match();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_pair() -> (NetLink, UdpSocket, Arc<Mutex<Heard>>) {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        a.set_nonblocking(true).unwrap();
        let heard = Arc::new(Mutex::new(Heard {
            since: Instant::now(),
            last: None,
            bye: false,
        }));
        let link = NetLink {
            peers: vec![b.local_addr().unwrap()],
            socket: a,
            heard: heard.clone(),
        };
        (link, b, heard)
    }

    #[test]
    fn the_link_hears_packets_and_a_goodbye_and_skips_the_punch() {
        let (mut link, peer, heard) = link_pair();
        let to = link.socket.local_addr().unwrap();
        peer.send_to(b"SMCP\x01\x05punch", to).unwrap();
        let p = Packet {
            player: 1,
            ..Default::default()
        };
        peer.send_to(&p.to_bytes(), to).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(link.poll().len(), 1);
        assert!(heard.lock().unwrap().last.is_some());
        assert!(!heard.lock().unwrap().bye);
        // a stranger's goodbye isn't the peer's
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        stranger.send_to(BYE, to).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        link.poll();
        assert!(!heard.lock().unwrap().bye);
        peer.send_to(BYE, to).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        link.poll();
        assert!(heard.lock().unwrap().bye);
    }
}

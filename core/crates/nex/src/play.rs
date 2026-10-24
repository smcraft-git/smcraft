//! Network Play: from a signed-in account to the addresses of the other players, for
//! `mmk-netplay`'s rollback session.
//!
//! 1. Bind the game's UDP socket; log in; open the secure connection.
//! 2. With a NAT check server: learn our public address and NAT type, report it
//!    (NATTraversal.ReportNATProperties).
//! 3. Register our station URLs (LAN address and the game socket's port; the public one when
//!    known).
//! 4. AutoMatchmake an SMCraft session (its own game mode, attributes and course): the server
//!    joins us to one that matches or opens ours. The owner is the host, player 1.
//! 5. A joiner asks for the host's station URLs (MatchMaking.GetSessionURLs) and for the
//!    server to have the host probe it back (NATTraversal.RequestProbeInitiationExt); both
//!    then punch (see [`crate::nat`]). When every player has answered, the host sends the
//!    player list (START) and closes the gathering to newcomers.
//! 6. The secure connection stays open in the background (pings) until the [`Match`] is
//!    dropped, which leaves the gathering.
//!
//! Nothing here logs a PID or a peer's address (Nextendo treats the PID as a credential).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::Error;
use crate::client::{self, Event, GameServer, Secure};
use crate::crypto::random_bytes;
use crate::matchmaking::*;
use crate::nat::{self, Message};
use crate::station::{StationUrl, TYPE_BEHIND_NAT, TYPE_PUBLIC};

pub type Log = Arc<dyn Fn(&str) + Send + Sync>;

pub struct PlayConfig {
    pub server: GameServer,
    /// The account (Nextendo: its PID) and the ID token carrying its game token.
    pub pid: u64,
    pub id_token: String,
    /// [`smcraft::KIND_COOP`] or [`smcraft::KIND_VERSUS`].
    pub kind: u32,
    /// Players in the match (2–4).
    pub players: u16,
    /// A hash of the course everyone plays.
    pub course: u64,
    /// Our course pick, if we open the session (see [`Match::pick`]).
    pub pick: Vec<u8>,
    /// Our player name, shown to the others (at most 32 bytes travel).
    pub name: String,
    /// Where the game's UDP socket binds (e.g. 0.0.0.0:0).
    pub bind: SocketAddr,
    /// Our LAN address in the station URL (default: the one that routes to the server).
    pub advertise: Option<Ipv4Addr>,
    /// A NAT check server (two UDP ports), to learn our public address first.
    pub nat_check: Option<(Ipv4Addr, [u16; 2])>,
    /// How long to wait for the other players.
    pub wait: Duration,
    pub log: Log,
    /// Set from another thread to give up the search: [`find_match`] leaves the session and
    /// returns [`Error::Cancelled`].
    pub cancel: Option<Arc<AtomicBool>>,
    /// The search's progress for a waiting screen: (players so far, us included; players
    /// wanted).
    pub progress: Option<Progress>,
}

pub type Progress = Arc<dyn Fn(usize, usize) + Send + Sync>;

fn cancelled(c: &Option<Arc<AtomicBool>>) -> bool {
    c.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
}

/// A match ready to play.
pub struct Match {
    /// The game's UDP socket (non-blocking), already punched through to the peers.
    pub socket: UdpSocket,
    /// Our player index (the host is 0) and the number of players.
    pub me: usize,
    pub players: usize,
    /// Every other player's address.
    pub peers: Vec<SocketAddr>,
    pub host: bool,
    pub gid: u32,
    /// The course pick of the player who opened the session (the same for everyone).
    pub pick: Vec<u8>,
    /// Every player's name by player index (empty where a peer sent none).
    pub names: Vec<String>,
    stop: Arc<AtomicBool>,
    keepalive: Option<JoinHandle<()>>,
}

impl Drop for Match {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.keepalive.take() {
            let _ = h.join();
        }
    }
}

/// The local address that routes to `to` (no packet is sent).
pub fn local_ip_toward(to: Ipv4Addr) -> Ipv4Addr {
    if to.is_loopback() {
        return Ipv4Addr::LOCALHOST;
    }
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect((to, 9))?;
            s.local_addr()
        })
        .ok()
        .and_then(|a| match a.ip() {
            IpAddr::V4(v) if !v.is_unspecified() => Some(v),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::LOCALHOST)
}

fn server_ip(url: &str) -> Option<Ipv4Addr> {
    let (_, host, port, _) = crate::ws::parse_url(url).ok()?;
    (host.as_str(), port)
        .to_socket_addrs()
        .ok()?
        .find_map(|a| match a.ip() {
            IpAddr::V4(v) => Some(v),
            _ => None,
        })
}

fn station(addr: SocketAddrV4, nat: Option<&nat::NatReport>, public: bool) -> StationUrl {
    let mut u = StationUrl::new("prudp");
    u.set("address", addr.ip())
        .set("port", addr.port())
        .set("Pl", 2)
        .set("natf", nat.map_or(0, |n| n.filtering))
        .set("natm", nat.map_or(0, |n| n.mapping))
        .set("pmp", 0)
        .set("sid", 15)
        .set("upnp", 0);
    if public {
        u.set("type", TYPE_BEHIND_NAT | TYPE_PUBLIC);
    }
    u
}

/// Finds (or opens) an SMCraft session and connects to its players.
pub fn find_match(cfg: PlayConfig) -> Result<Match, Error> {
    let log = cfg.log.clone();
    let socket = UdpSocket::bind(cfg.bind)?;
    let port = socket.local_addr()?.port();
    let server_v4 = server_ip(&cfg.server.auth_url).unwrap_or(Ipv4Addr::LOCALHOST);

    let login = client::login(&cfg.server, cfg.pid, &cfg.id_token)?;
    log(&format!(
        "Network Play: signed in to the game server ({})",
        login.server_name
    ));
    let mut secure = Secure::connect(&cfg.server, &login)?;

    let report = match cfg.nat_check {
        Some((ip, ports)) => {
            let r = nat::nat_check(&socket, ip, ports, Duration::from_secs(3))?;
            log(&format!(
                "Network Play: NAT check done ({})",
                if r.mapping == 1 {
                    "hole punching should work"
                } else {
                    "symmetric NAT: may not connect"
                }
            ));
            secure.report_nat_properties(r.mapping, r.filtering, r.rtt_ms)?;
            Some(r)
        }
        None => None,
    };
    let lan = SocketAddrV4::new(
        cfg.advertise.unwrap_or_else(|| local_ip_toward(server_v4)),
        port,
    );
    let mut urls = vec![station(lan, report.as_ref(), false)];
    if let Some(r) = &report
        && r.external != lan
    {
        urls.push(station(r.external, Some(r), true));
    }
    secure.register(&urls)?;
    // what we ask peers to probe: our best address, with our connection id
    let mut me_url = urls.last().unwrap().clone();
    me_url.set("PID", cfg.pid).set("RVCID", secure.rvcid);

    let players = cfg.players.clamp(2, 4);
    if cancelled(&cfg.cancel) {
        secure.disconnect();
        return Err(Error::Cancelled);
    }
    let session = secure.auto_matchmake(&AutoMatchmakeParam {
        session: MatchmakeSession::smcraft(cfg.kind, players, cfg.course).with_pick(&cfg.pick),
        criteria: vec![SearchCriteria::smcraft(cfg.kind, cfg.course)],
        participation_count: 1,
        join_message: "SMCraft".into(),
        ..Default::default()
    })?;
    if !session.is_smcraft() {
        return Err(Error::Protocol(
            "the server matched us with a session that isn't SMCraft's".into(),
        ));
    }
    if session.smcraft_course() != Some(cfg.course) {
        return Err(Error::Protocol("the session plays another course".into()));
    }
    let gid = session.id;
    let pick = session.smcraft_pick().to_vec();
    let host = session.owner == cfg.pid;
    socket.set_read_timeout(Some(Duration::from_millis(20)))?;
    let deadline = Instant::now() + cfg.wait;

    let report = |found: usize| {
        if let Some(p) = &cfg.progress {
            p(found, players as usize);
        }
    };
    let mut names: Vec<String> = Vec::new();
    let waited = if host {
        log(&format!(
            "Network Play: opened a session, waiting for {} more player(s)",
            players - 1
        ));
        report(1);
        host_wait(
            &mut secure,
            &socket,
            (cfg.pid, &cfg.name),
            gid,
            players as usize,
            deadline,
            &cfg.cancel,
            &report,
            &mut names,
        )
    } else {
        log("Network Play: joined a session, connecting to the host");
        report(2);
        joiner_connect(
            &mut secure,
            &socket,
            &cfg,
            &session,
            &me_url,
            deadline,
            &mut names,
        )
    };
    let (me, peers) = match waited {
        Ok(v) => v,
        Err(e) => {
            // leave the session, so nobody joins one that is gone
            if host {
                let _ = secure.unregister_gathering(gid);
            } else {
                let _ = secure.end_participation(gid, "");
            }
            secure.disconnect();
            return Err(e);
        }
    };
    log(&format!(
        "Network Play: {} players connected, we are player {}",
        peers.len() + 1,
        me + 1
    ));
    if host {
        let _ = secure.set_participation(gid, false);
    }
    socket.set_read_timeout(None)?;
    socket.set_nonblocking(true)?;

    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let keepalive = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            if secure.poll(Duration::from_millis(250)).is_err() {
                return;
            }
        }
        if host {
            let _ = secure.unregister_gathering(gid);
        } else {
            let _ = secure.end_participation(gid, "");
        }
        secure.disconnect();
    });
    Ok(Match {
        socket,
        me,
        players: peers.len() + 1,
        peers,
        host,
        gid,
        pick,
        names,
        stop,
        keepalive: Some(keepalive),
    })
}

fn nonce() -> u64 {
    u64::from_le_bytes(random_bytes(8).try_into().unwrap())
}

fn recv(socket: &UdpSocket) -> Option<(Message, SocketAddr)> {
    let mut buf = [0u8; 512];
    match socket.recv_from(&mut buf) {
        Ok((n, from)) => Message::decode(&buf[..n]).map(|m| (m, from)),
        Err(_) => None,
    }
}

/// A joiner: asks for the host's addresses and its probe, then punches until START.
fn joiner_connect(
    secure: &mut Secure,
    socket: &UdpSocket,
    cfg: &PlayConfig,
    session: &MatchmakeSession,
    me_url: &StationUrl,
    deadline: Instant,
    names: &mut Vec<String>,
) -> Result<(usize, Vec<SocketAddr>), Error> {
    let host_urls = secure.session_urls(session.id)?;
    // one probe request per connection id (the host's LAN and public URLs share it)
    let mut targets: Vec<StationUrl> = Vec::new();
    for u in &host_urls {
        if u.get("RVCID").is_some() && !targets.iter().any(|t| t.get("RVCID") == u.get("RVCID")) {
            targets.push(u.clone());
        }
    }
    secure.request_probe_initiation(&targets, me_url)?;
    let candidates: Vec<SocketAddr> = host_urls.iter().filter_map(|u| u.socket_addr()).collect();
    joiner_wait(
        secure,
        socket,
        (cfg.pid, &cfg.name),
        session.owner,
        session.id,
        &candidates,
        deadline,
        &cfg.cancel,
        names,
    )
}

/// The host: learns each joiner (Participate, InitiateProbe), punches, then sends START.
#[allow(clippy::too_many_arguments)]
fn host_wait(
    secure: &mut Secure,
    socket: &UdpSocket,
    (me, my_name): (u64, &str),
    gid: u32,
    players: usize,
    deadline: Instant,
    cancel: &Option<Arc<AtomicBool>>,
    report: &dyn Fn(usize),
    names: &mut Vec<String>,
) -> Result<(usize, Vec<SocketAddr>), Error> {
    // each joiner's name, from its PROBE / REPLY
    let mut joiner_names: HashMap<u64, String> = HashMap::new();
    // joiner PID → (candidate addresses, confirmed address), in the order they came
    let mut order: Vec<u64> = Vec::new();
    let mut candidates: HashMap<u64, Vec<SocketAddr>> = HashMap::new();
    let mut confirmed: HashMap<u64, SocketAddr> = HashMap::new();
    let mut next_probe = Instant::now();
    let probe_nonce = nonce();
    let mut reported = 0;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Timeout("waiting for the other players"));
        }
        if cancelled(cancel) {
            return Err(Error::Cancelled);
        }
        if order.len() != reported {
            reported = order.len();
            report((1 + reported).min(players));
        }
        for e in secure.poll(Duration::from_millis(30))? {
            match e {
                Event::Notification(n)
                    if n.kind == NOTIFY_PARTICIPATE && n.param1 == gid as u64 && n.param2 != me =>
                {
                    if !order.contains(&n.param2) {
                        order.push(n.param2);
                    }
                }
                Event::Notification(n)
                    if matches!(n.kind, NOTIFY_DISCONNECTED | NOTIFY_END_PARTICIPATION)
                        && n.param1 == gid as u64 =>
                {
                    order.retain(|p| *p != n.param2);
                    confirmed.remove(&n.param2);
                }
                Event::InitiateProbe(url) => {
                    if let (Some(pid), Some(addr)) = (url.get_u64("PID"), url.socket_addr()) {
                        if !order.contains(&pid) {
                            order.push(pid);
                        }
                        let c = candidates.entry(pid).or_default();
                        if !c.contains(&addr) {
                            c.push(addr);
                        }
                    }
                }
                _ => {}
            }
        }
        if Instant::now() >= next_probe {
            for (pid, addrs) in &candidates {
                if confirmed.contains_key(pid) {
                    continue;
                }
                let mut m = Message::new(nat::PROBE, gid, me);
                m.nonce = probe_nonce;
                m.name = my_name.to_string();
                for a in addrs {
                    let _ = socket.send_to(&m.encode(), a);
                }
            }
            next_probe = Instant::now() + Duration::from_millis(100);
        }
        while let Some((m, from)) = recv(socket) {
            if m.gid != gid || m.from == me {
                continue;
            }
            match m.kind {
                nat::PROBE => {
                    let mut r = Message::new(nat::REPLY, gid, me);
                    r.nonce = m.nonce;
                    r.name = my_name.to_string();
                    let _ = socket.send_to(&r.encode(), from);
                    if !order.contains(&m.from) {
                        order.push(m.from);
                    }
                    confirmed.insert(m.from, from);
                    joiner_names.insert(m.from, m.name);
                }
                nat::REPLY if m.nonce == probe_nonce => {
                    if !order.contains(&m.from) {
                        order.push(m.from);
                    }
                    confirmed.insert(m.from, from);
                    joiner_names.insert(m.from, m.name);
                }
                _ => {}
            }
        }
        let ready: Vec<u64> = order
            .iter()
            .filter(|p| confirmed.contains_key(p))
            .copied()
            .take(players - 1)
            .collect();
        if ready.len() == players - 1 {
            *names = std::iter::once(my_name.to_string())
                .chain(
                    ready
                        .iter()
                        .map(|p| joiner_names.get(p).cloned().unwrap_or_default()),
                )
                .collect();
            return start(socket, me, gid, &ready, &confirmed, deadline, cancel, names);
        }
    }
}

/// Sends START (with everyone's names) to every joiner until each says READY.
#[allow(clippy::too_many_arguments)]
fn start(
    socket: &UdpSocket,
    me: u64,
    gid: u32,
    joiners: &[u64],
    addrs: &HashMap<u64, SocketAddr>,
    deadline: Instant,
    cancel: &Option<Arc<AtomicBool>>,
    names: &[String],
) -> Result<(usize, Vec<SocketAddr>), Error> {
    let v4 = |a: &SocketAddr| match a {
        SocketAddr::V4(v) => *v,
        SocketAddr::V6(_) => SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0),
    };
    let mut m = Message::new(nat::START, gid, me);
    m.players
        .push((me, SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)));
    m.players
        .extend(joiners.iter().map(|p| (*p, v4(&addrs[p]))));
    m.names = names.to_vec();
    let bytes = m.encode();
    let mut ready: Vec<u64> = Vec::new();
    let mut next = Instant::now();
    let give_up = (Instant::now() + Duration::from_secs(5)).min(deadline + Duration::from_secs(5));
    while ready.len() < joiners.len() && Instant::now() < give_up {
        if cancelled(cancel) {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= next {
            for p in joiners.iter().filter(|p| !ready.contains(p)) {
                let _ = socket.send_to(&bytes, addrs[p]);
            }
            next = Instant::now() + Duration::from_millis(100);
        }
        while let Some((r, from)) = recv(socket) {
            match r.kind {
                nat::READY if r.gid == gid && !ready.contains(&r.from) => ready.push(r.from),
                nat::PROBE if r.gid == gid => {
                    let mut a = Message::new(nat::REPLY, gid, me);
                    a.nonce = r.nonce;
                    let _ = socket.send_to(&a.encode(), from);
                }
                _ => {}
            }
        }
    }
    if ready.len() < joiners.len() {
        return Err(Error::Timeout("the other players' READY"));
    }
    Ok((0, joiners.iter().map(|p| addrs[p]).collect()))
}

/// A joiner: punches to the host's candidates until START.
#[allow(clippy::too_many_arguments)]
fn joiner_wait(
    secure: &mut Secure,
    socket: &UdpSocket,
    (me, my_name): (u64, &str),
    owner: u64,
    gid: u32,
    candidates: &[SocketAddr],
    deadline: Instant,
    cancel: &Option<Arc<AtomicBool>>,
    names: &mut Vec<String>,
) -> Result<(usize, Vec<SocketAddr>), Error> {
    let probe_nonce = nonce();
    let mut next_probe = Instant::now();
    let mut host_addr: Option<SocketAddr> = None;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Timeout("the host's START"));
        }
        if cancelled(cancel) {
            return Err(Error::Cancelled);
        }
        secure.poll(Duration::from_millis(10))?;
        if Instant::now() >= next_probe {
            let mut m = Message::new(nat::PROBE, gid, me);
            m.nonce = probe_nonce;
            m.name = my_name.to_string();
            for a in host_addr.iter().chain(candidates) {
                let _ = socket.send_to(&m.encode(), a);
            }
            next_probe = Instant::now() + Duration::from_millis(100);
        }
        while let Some((m, from)) = recv(socket) {
            if m.gid != gid || m.from != owner {
                continue;
            }
            match m.kind {
                nat::PROBE => {
                    let mut r = Message::new(nat::REPLY, gid, me);
                    r.nonce = m.nonce;
                    r.name = my_name.to_string();
                    let _ = socket.send_to(&r.encode(), from);
                    host_addr = Some(from);
                }
                nat::REPLY => host_addr = Some(from),
                nat::START => {
                    let Some(idx) = m.players.iter().position(|(p, _)| *p == me) else {
                        continue;
                    };
                    *names = m.names.clone();
                    let ready = Message::new(nat::READY, gid, me).encode();
                    for _ in 0..3 {
                        let _ = socket.send_to(&ready, from);
                    }
                    let peers = m
                        .players
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != idx)
                        .map(|(i, (_, a))| if i == 0 { from } else { SocketAddr::V4(*a) })
                        .collect();
                    return Ok((idx, peers));
                }
                _ => {}
            }
        }
    }
}

//! A headless Network Play player for tests: signs in to a local NEX server (a test account),
//! matchmakes, punches through, then plays the demo strip over `mmk-netplay` at 60 frames a
//! second with a scripted run (right, jumping now and then), and reports how it went.
//!
//!     cargo run -p mmk-nex --example peer -- <auth ip:port> <access key> <token secret> [frames]
//!
//! Pair it with `mmk-play --nextendo-local <auth ip:port> --no-menus …` (same key and secret)
//! for a co-op run with one window.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mmk_netplay::{Link, Session, UdpLink};
use mmk_nex::matchmaking::smcraft;
use mmk_nex::play::{self, PlayConfig};
use mmk_nex::{GameServer, crypto, token};
use mmk_sim::{Input, World, input};

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        return Err("usage: peer <auth ip:port> <access key> <token secret> [frames]".into());
    }
    let frames: usize = a.get(3).and_then(|f| f.parse().ok()).unwrap_or(1200);
    let pid = 1_000_000
        + u64::from(u32::from_le_bytes(
            crypto::random_bytes(4).try_into().unwrap(),
        )) % 1_000_000_000;
    let nx2 = token::nx2_token_for_a_day(a[2].as_bytes(), pid, "peer");
    let m = play::find_match(PlayConfig {
        server: GameServer::smm2(&format!("ws://{}/", a[0]), &a[1]),
        pid,
        id_token: token::id_token(pid, &nx2, &crypto::random_bytes(32)),
        kind: smcraft::KIND_COOP,
        players: 2,
        course: 0,
        pick: Vec::new(),
        name: String::new(),
        bind: "0.0.0.0:0".parse().unwrap(),
        advertise: None,
        nat_check: None,
        wait: Duration::from_secs(300),
        log: Arc::new(|m| eprintln!("peer: {m}")),
        cancel: None,
        progress: None,
    })
    .map_err(|e| e.to_string())?;
    let mut world = World::new();
    world.set_players(m.players);
    let mut s = Session::new(world, m.players, m.me);
    let mut link = UdpLink::from_socket(
        m.socket.try_clone().map_err(|e| e.to_string())?,
        m.peers.clone(),
    )
    .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let tick = Duration::from_micros(16_667);
    let mut next = start;
    while s.frame < frames {
        for p in link.poll() {
            s.receive(&p);
        }
        if s.can_advance() {
            let f = s.frame;
            let mut b = input::RIGHT;
            if f % 90 < 20 {
                b |= input::JUMP;
            }
            s.advance(Input { buttons: b });
        }
        link.send(&s.packet());
        next += tick;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        if start.elapsed() > Duration::from_secs(120) {
            break;
        }
    }
    eprintln!(
        "peer: player {} of {}: {} frames in {:.1} s, {} rollbacks, {} state checks compared, desync {:?}",
        m.me + 1,
        m.players,
        s.frame,
        start.elapsed().as_secs_f32(),
        s.rollbacks,
        s.checks_compared,
        s.desync
    );
    Ok(())
}

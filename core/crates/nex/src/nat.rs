//! NAT traversal for SMCraft's peers.
//!
//! **The NAT check** ([`nat_check`]) is the one Switch games run against Nextendo's NAT check
//! servers (`nncs1` / `nncs2`, UDP 10025 and 10125): a 16-byte probe of four big-endian u32
//! (test id, 0, 0, our local IPv4) answered with (test id, the source port it saw, the source
//! IPv4 it saw, its own IPv4). Probing two ports from one socket tells whether the NAT keeps
//! one mapping for every destination (hole punching works) or makes a new one each time
//! (symmetric: it rarely does).
//!
//! **The hole punch** is SMCraft's own small protocol on the game's UDP socket, run once
//! matchmaking has named the peers (the joiner gets the host's station URLs from
//! GetSessionURLs, the host gets the joiner's from the InitiateProbe the server pushes):
//! datagrams that start with `SMCP`, so `mmk-netplay` skips them:
//!
//! ```text
//! "SMCP" u8 version 1, u8 kind, u32 gathering id, u64 sender PID, then by kind:
//!   1 PROBE  u64 nonce                          to every candidate address of a peer
//!   2 REPLY  u64 nonce (the probe's)            back to the address the probe came from
//!   3 START  u8 players, then per player: u64 PID, 4 bytes IPv4, u16 port (the host's view;
//!            the receiver's own entry is how the host sees it, the host's entry is 0.0.0.0)
//!   4 READY  (a joiner got START)
//! ```
//!
//! All little-endian after the kind.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use crate::Error;

pub const NAT_CHECK_PORTS: [u16; 2] = [10025, 10125];

/// What the NAT check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NatReport {
    /// Our address as the NAT check server saw it.
    pub external: SocketAddrV4,
    /// NAT mapping as Nextendo's matchmaking reads it: 1 one mapping for every destination,
    /// 2 a mapping per destination (symmetric).
    pub mapping: u32,
    /// NAT filtering: 0 not measured.
    pub filtering: u32,
    /// Round trip of the first probe, ms.
    pub rtt_ms: u32,
}

/// Probes `server` on both NAT check ports (UDP, from `socket`).
pub fn nat_check(
    socket: &UdpSocket,
    server: Ipv4Addr,
    ports: [u16; 2],
    timeout: Duration,
) -> Result<NatReport, Error> {
    let local = match socket.local_addr()? {
        SocketAddr::V4(a) => *a.ip(),
        SocketAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
    };
    let mut seen = Vec::new();
    let mut rtt_ms = 0;
    for (i, port) in ports.into_iter().enumerate() {
        let test = 101 + i as u32;
        let mut probe = Vec::with_capacity(16);
        for w in [test, 0, 0, u32::from(local)] {
            probe.extend_from_slice(&w.to_be_bytes());
        }
        let to = SocketAddrV4::new(server, port);
        let start = Instant::now();
        let deadline = start + timeout;
        let mut next_send = start;
        let answer = loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(Error::Timeout("NAT check"));
            }
            if now >= next_send {
                socket.send_to(&probe, to)?;
                next_send = now + Duration::from_millis(250);
            }
            socket.set_read_timeout(Some(
                (next_send.min(deadline) - now).max(Duration::from_millis(1)),
            ))?;
            let mut buf = [0u8; 64];
            match socket.recv_from(&mut buf) {
                Ok((16, SocketAddr::V4(from))) if from == to => {
                    let w = |i: usize| u32::from_be_bytes(buf[i..i + 4].try_into().unwrap());
                    if w(0) == test {
                        break (w(4) as u16, Ipv4Addr::from(w(8)));
                    }
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        };
        if i == 0 {
            rtt_ms = start.elapsed().as_millis() as u32;
        }
        seen.push(SocketAddrV4::new(answer.1, answer.0));
    }
    let mapping = if seen.iter().all(|a| *a == seen[0]) {
        1
    } else {
        2
    };
    Ok(NatReport {
        external: seen[0],
        mapping,
        filtering: 0,
        rtt_ms,
    })
}

const MAGIC: &[u8; 4] = b"SMCP";
pub const PROBE: u8 = 1;
pub const REPLY: u8 = 2;
pub const START: u8 = 3;
pub const READY: u8 = 4;

/// One handshake datagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: u8,
    pub gid: u32,
    pub from: u64,
    pub nonce: u64,
    /// START: (PID, address as the host sees it).
    pub players: Vec<(u64, SocketAddrV4)>,
    /// PROBE / REPLY: the sender's player name (empty from a build that sends none).
    pub name: String,
    /// START: every player's name, in the players' order.
    pub names: Vec<String>,
}

/// A name as it travels: at most 32 bytes of UTF-8, cut at a character.
fn put_name(b: &mut Vec<u8>, name: &str) {
    let mut end = name.len().min(32);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    b.push(end as u8);
    b.extend_from_slice(&name.as_bytes()[..end]);
}

/// A name and what follows it (None: nothing there, as from an older build).
fn take_name(b: &[u8]) -> Option<(String, &[u8])> {
    let n = *b.first()? as usize;
    let s = b.get(1..1 + n)?;
    Some((String::from_utf8_lossy(s).into_owned(), &b[1 + n..]))
}

impl Message {
    pub fn new(kind: u8, gid: u32, from: u64) -> Self {
        Self {
            kind,
            gid,
            from,
            nonce: 0,
            players: Vec::new(),
            name: String::new(),
            names: Vec::new(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        b.extend_from_slice(&[1, self.kind]);
        b.extend_from_slice(&self.gid.to_le_bytes());
        b.extend_from_slice(&self.from.to_le_bytes());
        match self.kind {
            PROBE | REPLY => {
                b.extend_from_slice(&self.nonce.to_le_bytes());
                put_name(&mut b, &self.name);
            }
            START => {
                b.push(self.players.len() as u8);
                for (pid, a) in &self.players {
                    b.extend_from_slice(&pid.to_le_bytes());
                    b.extend_from_slice(&a.ip().octets());
                    b.extend_from_slice(&a.port().to_le_bytes());
                }
                b.push(self.names.len() as u8);
                for n in &self.names {
                    put_name(&mut b, n);
                }
            }
            _ => {}
        }
        b
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < 18 || &b[..4] != MAGIC || b[4] != 1 {
            return None;
        }
        let mut m = Self::new(
            b[5],
            u32::from_le_bytes(b[6..10].try_into().ok()?),
            u64::from_le_bytes(b[10..18].try_into().ok()?),
        );
        let rest = &b[18..];
        match m.kind {
            PROBE | REPLY => {
                m.nonce = u64::from_le_bytes(rest.get(..8)?.try_into().ok()?);
                if let Some((name, _)) = take_name(&rest[8..]) {
                    m.name = name;
                }
            }
            START => {
                let n = *rest.first()? as usize;
                for e in rest.get(1..1 + 14 * n)?.chunks(14) {
                    let ip = Ipv4Addr::new(e[8], e[9], e[10], e[11]);
                    m.players.push((
                        u64::from_le_bytes(e[..8].try_into().ok()?),
                        SocketAddrV4::new(ip, u16::from_le_bytes([e[12], e[13]])),
                    ));
                }
                // the names follow (none from an older build)
                if let Some(&k) = rest.get(1 + 14 * n) {
                    let mut tail = &rest[2 + 14 * n..];
                    for _ in 0..k {
                        let Some((name, next)) = take_name(tail) else {
                            break;
                        };
                        m.names.push(name);
                        tail = next;
                    }
                }
            }
            READY => {}
            _ => return None,
        }
        Some(m)
    }
}

/// Whether a datagram is the handshake's (so not `mmk-netplay`'s).
pub fn is_handshake(b: &[u8]) -> bool {
    b.starts_with(MAGIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let mut m = Message::new(PROBE, 7, 99);
        m.nonce = 0xDEAD_BEEF;
        assert_eq!(Message::decode(&m.encode()), Some(m));
        let mut s = Message::new(START, 7, 1);
        s.players = vec![
            (1, SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            (2, "10.0.0.2:7000".parse().unwrap()),
        ];
        assert!(is_handshake(&s.encode()));
        assert_eq!(Message::decode(&s.encode()), Some(s.clone()));
        // names: the sender's on PROBE / REPLY, everyone's on START, cut to 32 bytes
        let mut p = Message::new(REPLY, 7, 99);
        p.name = "Ada".into();
        assert_eq!(Message::decode(&p.encode()), Some(p));
        s.names = vec!["Ada".into(), "é".repeat(20)];
        let back = Message::decode(&s.encode()).unwrap();
        assert_eq!(back.names[0], "Ada");
        assert_eq!(back.names[1], "é".repeat(16), "32 bytes at most");
        // an older build's PROBE (no name) still reads
        let mut old = Message::new(PROBE, 7, 99).encode();
        old.pop();
        assert_eq!(Message::decode(&old).map(|m| m.name), Some(String::new()));
        assert_eq!(Message::decode(b"SMCP\x02"), None);
    }

    /// A NAT check responder on loopback (the protocol above), probed from one socket.
    #[test]
    fn nat_check_against_a_local_responder() {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ports = [
            a.local_addr().unwrap().port(),
            b.local_addr().unwrap().port(),
        ];
        for s in [a, b] {
            std::thread::spawn(move || {
                let mut buf = [0u8; 64];
                while let Ok((n, SocketAddr::V4(from))) = s.recv_from(&mut buf) {
                    if n != 16 {
                        continue;
                    }
                    let mut r = buf[..4].to_vec();
                    r.extend_from_slice(&(from.port() as u32).to_be_bytes());
                    r.extend_from_slice(&from.ip().octets());
                    r.extend_from_slice(&[127, 0, 0, 1]);
                    let _ = s.send_to(&r, from);
                }
            });
        }
        let me = UdpSocket::bind("127.0.0.1:0").unwrap();
        let report = nat_check(&me, Ipv4Addr::LOCALHOST, ports, Duration::from_secs(3)).unwrap();
        assert_eq!(SocketAddr::V4(report.external), me.local_addr().unwrap());
        assert_eq!(report.mapping, 1);
    }
}

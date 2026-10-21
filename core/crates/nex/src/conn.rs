//! A PRUDP Lite connection over a WebSocket, as the Switch client runs it (seen from SMM2 3.0.3
//! talking to a local test server, and from the server code):
//!
//! - SYN (NEED_ACK, sequence 0, the support option `6`) → the server's SYN-ACK carries its
//!   connection signature;
//! - CONNECT (RELIABLE | NEED_ACK, sequence 1, support + the Lite signature proving the access
//!   key, the login payload) → CONNECT-ACK with the server's answer;
//! - DATA: RELIABLE | NEED_ACK, sequence ids from 2, a message split into fragments of at most
//!   1300 bytes numbered 1, 2, … and 0 for the last (0 alone: a single fragment). Every reliable
//!   packet from the server is acknowledged (DATA | ACK with its sequence and fragment id) and
//!   delivered in sequence order, once;
//! - PING with NEED_ACK when idle (the game pings about every 10 s), DISCONNECT at the end.
//!
//! Both ends use stream type 10 (RVSecure). The authentication connection is port 31 → 1, the
//! secure one 30 → 2 (the game's own ports).

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::Error;
use crate::prudp::lite::{self, Packet};
use crate::prudp::*;
use crate::ws::WebSocket;

/// The support option the game sends (minor version 6, no extra functions).
pub const MINOR_VERSION: u8 = 6;
pub const FRAGMENT_SIZE: usize = 1300;
/// Idle time before a ping.
pub const PING_EVERY: Duration = Duration::from_secs(5);
/// Unacknowledged reliable packets are sent again after this.
pub const RESEND_AFTER: Duration = Duration::from_secs(2);

pub struct Conn {
    ws: WebSocket,
    /// Our port and the server's.
    src: u8,
    dst: u8,
    seq: u16,
    ping_seq: u16,
    next_in: u16,
    held: HashMap<u16, Packet>,
    fragments: Vec<u8>,
    unacked: Vec<(u16, Vec<u8>, Instant)>,
    inbox: VecDeque<Vec<u8>>,
    raw: Vec<u8>,
    last_sent: Instant,
    closed: bool,
}

/// `a` comes before `b` in sequence order (with wrap-around).
fn before(a: u16, b: u16) -> bool {
    (a.wrapping_sub(b) as i16) < 0
}

impl Conn {
    fn packet(&self, kind: u8, flags: u16, seq: u16) -> Packet {
        Packet {
            kind,
            flags,
            src_type: STREAM_RV_SECURE,
            src_port: self.src,
            dst_type: STREAM_RV_SECURE,
            dst_port: self.dst,
            seq,
            ..Default::default()
        }
    }

    fn write(&mut self, p: &Packet) -> Result<Vec<u8>, Error> {
        let b = p.encode();
        self.ws.send(&b)?;
        self.last_sent = Instant::now();
        Ok(b)
    }

    /// Opens the connection: the handshake, with `payload` in the CONNECT; returns the
    /// server's CONNECT answer.
    pub fn connect(
        ws: WebSocket,
        access_key: &str,
        src: u8,
        dst: u8,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<(Self, Vec<u8>), Error> {
        let mut c = Self {
            ws,
            src,
            dst,
            seq: 2,
            ping_seq: 1,
            next_in: 1,
            held: HashMap::new(),
            fragments: Vec::new(),
            unacked: Vec::new(),
            inbox: VecDeque::new(),
            raw: Vec::new(),
            last_sent: Instant::now(),
            closed: false,
        };
        let deadline = Instant::now() + timeout;
        let mut syn = c.packet(SYN, FLAG_NEED_ACK, 0);
        syn.options = vec![lite::support(MINOR_VERSION, 0)];
        let sig = c
            .handshake_step(&syn, SYN, deadline)?
            .option(OPT_CONNECTION_SIG)
            .ok_or_else(|| Error::Protocol("SYN-ACK without a connection signature".into()))?
            .to_vec();
        let mut connect = c.packet(CONNECT, FLAG_RELIABLE | FLAG_NEED_ACK, 1);
        connect.options = vec![
            lite::support(MINOR_VERSION, 0),
            (
                OPT_LITE_SIGNATURE,
                lite_connect_signature(access_key, &sig).to_vec(),
            ),
        ];
        connect.payload = payload.to_vec();
        let ack = c.handshake_step(&connect, CONNECT, deadline)?;
        Ok((c, ack.payload))
    }

    /// Sends `p` (again every second) until the server acknowledges it with a packet of `kind`.
    fn handshake_step(&mut self, p: &Packet, kind: u8, deadline: Instant) -> Result<Packet, Error> {
        loop {
            self.write(p)?;
            let resend = Instant::now() + Duration::from_secs(1);
            while Instant::now() < resend.min(deadline) {
                let wait = resend
                    .min(deadline)
                    .saturating_duration_since(Instant::now());
                let Some(frame) = self.ws.recv(wait)? else {
                    break;
                };
                self.raw.extend_from_slice(&frame);
                let mut got = lite::decode_all(&mut self.raw)?.into_iter();
                while let Some(q) = got.next() {
                    if q.kind == kind && q.has(FLAG_ACK) && q.dst_port == self.src {
                        // anything right behind the answer is handled as usual
                        for rest in got {
                            self.handle(rest)?;
                        }
                        return Ok(q);
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout(if kind == SYN {
                    "PRUDP SYN"
                } else {
                    "PRUDP CONNECT (wrong access key, or the login was refused)"
                }));
            }
        }
    }

    /// Sends one message (an RMC call or answer), in fragments when it is long.
    pub fn send(&mut self, data: &[u8]) -> Result<(), Error> {
        let chunks: Vec<&[u8]> = if data.is_empty() {
            vec![data]
        } else {
            data.chunks(FRAGMENT_SIZE).collect()
        };
        let n = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut p = self.packet(DATA, FLAG_RELIABLE | FLAG_NEED_ACK, self.seq);
            p.fragment = if i + 1 == n { 0 } else { (i + 1) as u8 };
            p.payload = chunk.to_vec();
            self.seq = self.seq.wrapping_add(1);
            let b = self.write(&p)?;
            self.unacked.push((p.seq, b, Instant::now()));
        }
        Ok(())
    }

    /// The next whole message from the server, waiting at most `timeout`.
    pub fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(m) = self.inbox.pop_front() {
                return Ok(Some(m));
            }
            if self.closed {
                return Err(Error::Closed);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.pump(left.min(Duration::from_millis(500)))?;
        }
    }

    /// Reads what has arrived (waiting at most `timeout`), answers acks and pings, resends what
    /// wasn't acknowledged and pings when idle.
    pub fn pump(&mut self, timeout: Duration) -> Result<(), Error> {
        if let Some(frame) = self.ws.recv(timeout)? {
            self.raw.extend_from_slice(&frame);
            for p in lite::decode_all(&mut self.raw)? {
                self.handle(p)?;
            }
        }
        let now = Instant::now();
        let mut again = Vec::new();
        for (_, b, at) in &mut self.unacked {
            if now.duration_since(*at) >= RESEND_AFTER {
                *at = now;
                again.push(b.clone());
            }
        }
        for b in again {
            self.ws.send(&b)?;
        }
        if now.duration_since(self.last_sent) >= PING_EVERY {
            let p = self.packet(PING, FLAG_NEED_ACK, self.ping_seq);
            self.ping_seq = self.ping_seq.wrapping_add(1);
            self.write(&p)?;
        }
        Ok(())
    }

    fn ack(&mut self, p: &Packet) -> Result<(), Error> {
        let mut a = self.packet(p.kind, FLAG_ACK, p.seq);
        a.fragment = p.fragment;
        self.write(&a).map(|_| ())
    }

    fn handle(&mut self, p: Packet) -> Result<(), Error> {
        if p.dst_port != self.src {
            return Ok(());
        }
        match p.kind {
            DATA if p.has(FLAG_MULTI_ACK) => {
                // [substream u8][extra count u8][base u16][extra u16 …]: everything up to base
                if p.payload.len() >= 4 {
                    let base = u16::from_le_bytes([p.payload[2], p.payload[3]]);
                    let extra: Vec<u16> = p.payload[4..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_le_bytes(*c))
                        .collect();
                    self.unacked
                        .retain(|(s, ..)| before(base, *s) && !extra.contains(s));
                }
            }
            DATA if p.has(FLAG_ACK) => self.unacked.retain(|(s, ..)| *s != p.seq),
            DATA => {
                if p.has(FLAG_NEED_ACK) {
                    self.ack(&p)?;
                }
                if !p.has(FLAG_RELIABLE) {
                    self.inbox.push_back(p.payload);
                    return Ok(());
                }
                if before(p.seq, self.next_in) {
                    return Ok(()); // already delivered: a resend
                }
                self.held.insert(p.seq, p);
                while let Some(q) = self.held.remove(&self.next_in) {
                    self.next_in = self.next_in.wrapping_add(1);
                    self.fragments.extend_from_slice(&q.payload);
                    if q.fragment == 0 {
                        self.inbox.push_back(std::mem::take(&mut self.fragments));
                    }
                }
            }
            PING if !p.has(FLAG_ACK) && p.has(FLAG_NEED_ACK) => self.ack(&p)?,
            DISCONNECT if !p.has(FLAG_ACK) => {
                if p.has(FLAG_NEED_ACK) {
                    self.ack(&p)?;
                }
                self.closed = true;
            }
            _ => {}
        }
        Ok(())
    }

    /// Says goodbye (DISCONNECT, waiting briefly for its ack) and closes the socket.
    pub fn disconnect(mut self) {
        let p = self.packet(DISCONNECT, FLAG_RELIABLE | FLAG_NEED_ACK, self.seq);
        if self.write(&p).is_ok() {
            let end = Instant::now() + Duration::from_millis(300);
            while Instant::now() < end {
                let Ok(Some(frame)) = self.ws.recv(end.saturating_duration_since(Instant::now()))
                else {
                    break;
                };
                self.raw.extend_from_slice(&frame);
                if let Ok(ps) = lite::decode_all(&mut self.raw)
                    && ps.iter().any(|q| q.kind == DISCONNECT && q.has(FLAG_ACK))
                {
                    break;
                }
            }
        }
        self.ws.close();
    }

    /// Reliable packets sent and not acknowledged yet.
    pub fn in_flight(&self) -> usize {
        self.unacked.len()
    }
}

#[cfg(test)]
mod tests {
    use super::before;

    #[test]
    fn sequence_order_wraps() {
        assert!(before(1, 2));
        assert!(!before(2, 2));
        assert!(before(65_535, 0));
        assert!(!before(0, 65_535));
    }
}

//! A WebSocket client (RFC 6455), enough for NEX: the HTTP/1.1 upgrade offering the `NEX`
//! subprotocol, masked binary frames out, binary / continuation / ping / close frames in.
//!
//! Plain `ws://` runs over TCP (the local test server); `wss://` (Nextendo's servers, behind
//! TLS on :443) needs a TLS [`Transport`] from the caller, see [`WebSocket::over`].

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::Error;
use crate::crypto::{b64, random_bytes, sha1};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// A byte stream with a read timeout (TCP, or TLS over TCP).
pub trait Transport: Read + Write + Send {
    fn set_read_timeout(&mut self, t: Option<Duration>) -> io::Result<()>;
}

impl Transport for TcpStream {
    fn set_read_timeout(&mut self, t: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, t)
    }
}

/// `ws://host:port/path` → (host, port, path).
pub fn parse_url(url: &str) -> Result<(bool, String, u16, String), Error> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("ws://") {
        (false, r)
    } else if let Some(r) = url.strip_prefix("wss://") {
        (true, r)
    } else {
        return Err(Error::Protocol(format!("not a ws:// or wss:// URL: {url}")));
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (
            h,
            p.parse()
                .map_err(|_| Error::Protocol(format!("bad port in {url}")))?,
        ),
        None => (hostport, if tls { 443 } else { 80 }),
    };
    Ok((tls, host.to_string(), port, path.to_string()))
}

pub struct WebSocket {
    io: Box<dyn Transport>,
    buf: Vec<u8>,
    /// The fragments of a message still coming.
    partial: Vec<u8>,
}

impl WebSocket {
    /// Connects to a `ws://` URL over TCP.
    pub fn connect(url: &str, timeout: Duration) -> Result<Self, Error> {
        let (tls, host, port, path) = parse_url(url)?;
        if tls {
            return Err(Error::Protocol(
                "wss:// needs a TLS transport (WebSocket::over)".into(),
            ));
        }
        let addr = (host.as_str(), port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::Protocol(format!("{host}: no address")))?;
        let tcp = TcpStream::connect_timeout(&addr, timeout)?;
        tcp.set_nodelay(true)?;
        tcp.set_write_timeout(Some(timeout))?;
        Self::over(Box::new(tcp), &format!("{host}:{port}"), &path, timeout)
    }

    /// Runs the upgrade over an open transport (`host` goes in the Host header).
    pub fn over(
        mut io: Box<dyn Transport>,
        host: &str,
        path: &str,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let key = b64(&random_bytes(16));
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: NEX\r\n\r\n"
        );
        io.write_all(req.as_bytes())?;
        io.flush()?;
        let deadline = Instant::now() + timeout;
        let mut buf = Vec::new();
        let end = loop {
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            if buf.len() > 16 * 1024 {
                return Err(Error::Protocol(
                    "WebSocket upgrade: headers too long".into(),
                ));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout("WebSocket upgrade"));
            }
            io.set_read_timeout(Some(left))?;
            let mut chunk = [0u8; 2048];
            match io.read(&mut chunk)? {
                0 => return Err(Error::Closed),
                n => buf.extend_from_slice(&chunk[..n]),
            }
        };
        let head = String::from_utf8_lossy(&buf[..end]).into_owned();
        let status = head.lines().next().unwrap_or_default();
        if !status.contains(" 101") {
            return Err(Error::Protocol(format!(
                "WebSocket upgrade refused: {status}"
            )));
        }
        let accept = b64(&sha1(format!("{key}{GUID}").as_bytes()));
        let ok = head.lines().any(|l| {
            l.split_once(':').is_some_and(|(k, v)| {
                k.trim().eq_ignore_ascii_case("sec-websocket-accept") && v.trim() == accept
            })
        });
        if !ok {
            return Err(Error::Protocol(
                "WebSocket upgrade: wrong accept key".into(),
            ));
        }
        Ok(Self {
            io,
            buf: buf[end..].to_vec(),
            partial: Vec::new(),
        })
    }

    fn frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), Error> {
        let mut f = Vec::with_capacity(payload.len() + 14);
        f.push(0x80 | opcode);
        match payload.len() {
            n if n < 126 => f.push(0x80 | n as u8),
            n if n < 65_536 => {
                f.push(0x80 | 126);
                f.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                f.push(0x80 | 127);
                f.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        let mask = random_bytes(4);
        f.extend_from_slice(&mask);
        f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i & 3]));
        self.io.write_all(&f)?;
        self.io.flush()?;
        Ok(())
    }

    pub fn send(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.frame(2, payload)
    }

    /// The next binary message, waiting at most `timeout` (None: nothing came).
    pub fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            while let Some((fin, opcode, payload, used)) = parse_frame(&self.buf)? {
                self.buf.drain(..used);
                match opcode {
                    0..=2 => {
                        self.partial.extend_from_slice(&payload);
                        if fin {
                            return Ok(Some(std::mem::take(&mut self.partial)));
                        }
                    }
                    8 => {
                        let _ = self.frame(8, &payload);
                        return Err(Error::Closed);
                    }
                    9 => self.frame(10, &payload)?,
                    _ => {}
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.io.set_read_timeout(Some(left))?;
            let mut chunk = [0u8; 16 * 1024];
            match self.io.read(&mut chunk) {
                Ok(0) => return Err(Error::Closed),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    pub fn close(&mut self) {
        let _ = self.frame(8, &1000u16.to_be_bytes());
    }
}

/// A frame: fin, opcode, the unmasked payload, the bytes it took.
type Frame = (bool, u8, Vec<u8>, usize);

/// One whole frame at the start of `b`.
fn parse_frame(b: &[u8]) -> Result<Option<Frame>, Error> {
    if b.len() < 2 {
        return Ok(None);
    }
    let (fin, opcode) = (b[0] & 0x80 != 0, b[0] & 0x0F);
    let masked = b[1] & 0x80 != 0;
    let (len, mut at) = match b[1] & 0x7F {
        126 if b.len() >= 4 => (u16::from_be_bytes([b[2], b[3]]) as usize, 4),
        127 if b.len() >= 10 => (
            u64::from_be_bytes(b[2..10].try_into().unwrap()) as usize,
            10,
        ),
        126 | 127 => return Ok(None),
        n => (n as usize, 2),
    };
    if len > 1 << 24 {
        return Err(Error::Protocol("WebSocket frame too large".into()));
    }
    let mask = if masked {
        if b.len() < at + 4 {
            return Ok(None);
        }
        at += 4;
        Some(&b[at - 4..at])
    } else {
        None
    };
    if b.len() < at + len {
        return Ok(None);
    }
    let mut payload = b[at..at + len].to_vec();
    if let Some(m) = mask {
        for (i, x) in payload.iter_mut().enumerate() {
            *x ^= m[i & 3];
        }
    }
    Ok(Some((fin, opcode, payload, at + len)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn urls() {
        assert_eq!(
            parse_url("ws://127.0.0.1:9/").unwrap(),
            (false, "127.0.0.1".into(), 9, "/".into())
        );
        assert_eq!(
            parse_url("wss://example.org").unwrap(),
            (true, "example.org".into(), 443, "/".into())
        );
        assert!(parse_url("http://x").is_err());
    }

    /// A one-connection echo server: checks the upgrade, answers, echoes one message in two
    /// fragments, then pings and closes.
    #[test]
    fn upgrade_echo_ping_close() {
        let ln = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ln.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = ln.accept().unwrap();
            let mut head = Vec::new();
            let mut b = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut b).unwrap();
                head.push(b[0]);
            }
            let head = String::from_utf8(head).unwrap();
            assert!(head.contains("Sec-WebSocket-Protocol: NEX"));
            let key = head
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap();
            let accept = b64(&sha1(format!("{key}{GUID}").as_bytes()));
            write!(
                s,
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Accept: {accept}\r\n\r\n"
            )
            .unwrap();
            // read the client's masked frame
            let mut h = [0u8; 2];
            s.read_exact(&mut h).unwrap();
            assert_eq!(h[0], 0x82);
            assert!(h[1] & 0x80 != 0, "client frames are masked");
            let n = (h[1] & 0x7F) as usize;
            let mut rest = vec![0u8; 4 + n];
            s.read_exact(&mut rest).unwrap();
            let msg: Vec<u8> = (0..n).map(|i| rest[4 + i] ^ rest[i & 3]).collect();
            // echo it in two fragments, then ping, then close
            s.write_all(&[0x02, 2, msg[0], msg[1]]).unwrap();
            s.write_all(&[0x80, (n - 2) as u8]).unwrap();
            s.write_all(&msg[2..]).unwrap();
            s.write_all(&[0x89, 1, b'p']).unwrap();
            let mut pong = [0u8; 7];
            s.read_exact(&mut pong).unwrap();
            assert_eq!(pong[0], 0x8A);
            s.write_all(&[0x88, 0]).unwrap();
        });
        let mut ws =
            WebSocket::connect(&format!("ws://127.0.0.1:{port}/"), Duration::from_secs(5)).unwrap();
        ws.send(b"hello nex").unwrap();
        assert_eq!(
            ws.recv(Duration::from_secs(5)).unwrap().as_deref(),
            Some(&b"hello nex"[..])
        );
        assert!(matches!(
            ws.recv(Duration::from_secs(5)),
            Err(Error::Closed)
        ));
        server.join().unwrap();
    }
}

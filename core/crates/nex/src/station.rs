//! Station URLs: how NEX names an endpoint, `scheme:/key=value;key=value`
//! (e.g. `prudps:/address=127.0.0.1;port=1;CID=1;PID=2;sid=1;stream=10;type=2`).
//! The order of the parameters is kept, so a URL round-trips byte for byte.

use std::fmt;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// Bits of a station's `type`.
pub const TYPE_BEHIND_NAT: u32 = 1;
pub const TYPE_PUBLIC: u32 = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StationUrl {
    pub scheme: String,
    params: Vec<(String, String)>,
}

impl StationUrl {
    pub fn new(scheme: &str) -> Self {
        Self {
            scheme: scheme.into(),
            params: Vec::new(),
        }
    }

    /// Parses `scheme:/k=v;…`; anything else is an empty URL.
    pub fn parse(s: &str) -> Self {
        let Some((scheme, fields)) = s.split_once(":/") else {
            return Self::default();
        };
        let mut u = Self::new(scheme);
        for f in fields.split(';') {
            if let Some((k, v)) = f.split_once('=') {
                u.set(k, v);
            }
        }
        u
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    }

    pub fn get_u64(&self, k: &str) -> Option<u64> {
        self.get(k)?.parse().ok()
    }

    /// Sets a parameter, keeping its place when it was already there.
    pub fn set(&mut self, k: &str, v: impl ToString) -> &mut Self {
        let v = v.to_string();
        match self.params.iter_mut().find(|(key, _)| key == k) {
            Some(p) => p.1 = v,
            None => self.params.push((k.into(), v)),
        }
        self
    }

    pub fn remove(&mut self, k: &str) -> &mut Self {
        self.params.retain(|(key, _)| key != k);
        self
    }

    /// `address` and `port` as an IPv4 socket address.
    pub fn socket_addr(&self) -> Option<SocketAddr> {
        let ip: Ipv4Addr = self.get("address")?.parse().ok()?;
        let port: u16 = self.get("port")?.parse().ok()?;
        Some(SocketAddr::V4(SocketAddrV4::new(ip, port)))
    }

    pub fn is_empty(&self) -> bool {
        self.scheme.is_empty() && self.params.is_empty()
    }
}

impl fmt::Display for StationUrl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:/", self.scheme)?;
        for (i, (k, v)) in self.params.iter().enumerate() {
            write!(f, "{}{k}={v}", if i > 0 { ";" } else { "" })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_order() {
        let s = "prudps:/address=127.0.0.1;port=5;CID=1;PID=2;sid=1;stream=10;type=2";
        let mut u = StationUrl::parse(s);
        assert_eq!(u.to_string(), s);
        assert_eq!(u.get_u64("PID"), Some(2));
        assert_eq!(u.socket_addr(), Some("127.0.0.1:5".parse().unwrap()));
        u.set("port", 9).set("RVCID", 3).remove("CID");
        assert_eq!(
            u.to_string(),
            "prudps:/address=127.0.0.1;port=9;PID=2;sid=1;stream=10;type=2;RVCID=3"
        );
        assert!(StationUrl::parse("garbage").is_empty());
    }
}

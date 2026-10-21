//! NEX's Kerberos-style tickets.
//!
//! The authentication server hands the client a ticket encrypted with the client's key. On
//! Nextendo (NEX 4) that key comes back with the login as `pSourceKey`, in hex; older servers
//! derive it from the account password. The ticket holds the session key, the secure server's
//! PID and an inner blob only the secure server can open (encrypted with its own key), which
//! the client passes on in its CONNECT.
//!
//! "Encrypted" is RC4 with the key, followed by an HMAC-MD5 of the ciphertext under the same key.
//! Ticket version 1 wraps the inner blob: [Buffer per-ticket key][Buffer data], the data keyed
//! with MD5(server key || per-ticket key).

use crate::Error;
use crate::crypto::{hmac_md5, md5, random_bytes, rc4, same};
use crate::stream::{DateTime, Reader, Writer};

/// The session key size on the Switch.
pub const SESSION_KEY_SIZE: usize = 32;

/// How a password becomes a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Derivation {
    /// MD5 repeated 65000 + pid % 1024 times (3DS / Wii U).
    Old,
    /// MD5 once, the PID appended (u64), MD5 once (Switch).
    New,
}

pub fn derive_key(password: &[u8], pid: u64, how: Derivation) -> Vec<u8> {
    match how {
        Derivation::Old => {
            let mut key = password.to_vec();
            for _ in 0..65_000 + pid % 1024 {
                key = md5(&key).to_vec();
            }
            key
        }
        Derivation::New => {
            let mut key = md5(password).to_vec();
            key.extend_from_slice(&pid.to_le_bytes());
            md5(&key).to_vec()
        }
    }
}

pub fn encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = rc4(key, data);
    let mac = hmac_md5(key, &out);
    out.extend_from_slice(&mac);
    out
}

pub fn decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() < 16 {
        return Err(Error::Protocol("ticket too short".into()));
    }
    let (body, mac) = data.split_at(data.len() - 16);
    if !same(mac, &hmac_md5(key, body)) {
        return Err(Error::Protocol(
            "ticket checksum does not match the key".into(),
        ));
    }
    Ok(rc4(key, body))
}

/// The ticket the client opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientTicket {
    pub session_key: Vec<u8>,
    /// The secure server's PID.
    pub target: u64,
    /// The blob for the secure server, sent in the CONNECT.
    pub internal: Vec<u8>,
}

impl ClientTicket {
    pub fn decrypt(key: &[u8], data: &[u8]) -> Result<Self, Error> {
        let plain = decrypt(key, data)?;
        let mut r = Reader::new(&plain);
        Ok(Self {
            session_key: r.take(SESSION_KEY_SIZE)?.to_vec(),
            target: r.pid()?,
            internal: r.buffer()?.to_vec(),
        })
    }

    pub fn encrypt(&self, key: &[u8]) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.session_key)
            .pid(self.target)
            .buffer(&self.internal);
        encrypt(key, w.bytes())
    }
}

/// The blob inside the ticket, as the secure server sees it (built here for the tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerTicket {
    pub time: DateTime,
    pub source: u64,
    pub session_key: Vec<u8>,
}

impl ServerTicket {
    /// Ticket version 1.
    pub fn encrypt(&self, server_key: &[u8]) -> Vec<u8> {
        let mut body = Writer::new();
        body.datetime(self.time)
            .pid(self.source)
            .raw(&self.session_key);
        let ticket_key = random_bytes(16);
        let key = md5(&[server_key, &ticket_key].concat());
        let mut w = Writer::new();
        w.buffer(&ticket_key).buffer(&encrypt(&key, body.bytes()));
        w.into_bytes()
    }

    pub fn decrypt(server_key: &[u8], data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let ticket_key = r.buffer()?;
        let key = md5(&[server_key, ticket_key].concat());
        let plain = decrypt(&key, r.buffer()?)?;
        let mut r = Reader::new(&plain);
        Ok(Self {
            time: r.datetime()?,
            source: r.pid()?,
            session_key: r.take(SESSION_KEY_SIZE)?.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::hex;

    #[test]
    fn derivations() {
        // MD5(MD5("pw") || 2u64)
        let mut k = md5(b"pw").to_vec();
        k.extend_from_slice(&2u64.to_le_bytes());
        assert_eq!(derive_key(b"pw", 2, Derivation::New), md5(&k));
        assert_eq!(derive_key(b"pw", 2, Derivation::Old).len(), 16);
    }

    #[test]
    fn tickets_round_trip_and_reject_a_wrong_key() {
        let server_key = derive_key(b"secure", 2, Derivation::New);
        let session_key = random_bytes(SESSION_KEY_SIZE);
        let inner = ServerTicket {
            time: DateTime::now(),
            source: 1234,
            session_key: session_key.clone(),
        };
        let internal = inner.encrypt(&server_key);
        assert_eq!(
            ServerTicket::decrypt(&server_key, &internal).unwrap(),
            inner
        );
        let t = ClientTicket {
            session_key,
            target: 2,
            internal,
        };
        let source_key = b"nextendo-srckey1";
        let enc = t.encrypt(source_key);
        assert_eq!(ClientTicket::decrypt(source_key, &enc).unwrap(), t);
        assert!(ClientTicket::decrypt(b"another-key-1234", &enc).is_err());
        assert_eq!(hex(&encrypt(b"k", b"")).len(), 32);
    }
}

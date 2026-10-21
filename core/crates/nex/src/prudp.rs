//! PRUDP packets.
//!
//! **Lite** (`lite`) is what Switch games speak to Nextendo's game servers: one packet per
//! WebSocket binary frame, no PRUDP encryption (TLS carries it), no session id, and a signature
//! only on the CONNECT. Header (12 bytes, little-endian):
//!
//! ```text
//! u8 0x80 | u8 option length | u16 payload length | u8 source type << 4 | destination type
//! u8 source port | u8 destination port | u8 fragment id | u16 type | flags << 4 | u16 sequence id
//! ```
//!
//! then the options (`id, length, value`) and the payload.
//!
//! **V1** (`v1`) is the older raw-UDP format (magic `EA D0`, a 16-byte HMAC-MD5 signature on
//! every packet). Nextendo's servers keep it for one non-Switch-native title; it's here as a
//! codec so the same client can speak it, but no Switch game server uses it.

use crate::Error;
use crate::crypto::{hmac_md5, md5};

pub const SYN: u8 = 0;
pub const CONNECT: u8 = 1;
pub const DATA: u8 = 2;
pub const DISCONNECT: u8 = 3;
pub const PING: u8 = 4;

pub const FLAG_ACK: u16 = 0x001;
pub const FLAG_RELIABLE: u16 = 0x002;
pub const FLAG_NEED_ACK: u16 = 0x004;
pub const FLAG_HAS_SIZE: u16 = 0x008;
pub const FLAG_MULTI_ACK: u16 = 0x200;

/// The stream type both ends use ("RVSecure").
pub const STREAM_RV_SECURE: u8 = 10;

pub const OPT_SUPPORT: u8 = 0;
pub const OPT_CONNECTION_SIG: u8 = 1;
pub const OPT_FRAGMENT_ID: u8 = 2;
pub const OPT_UNRELIABLE_SEQ: u8 = 3;
pub const OPT_MAX_SUBSTREAM: u8 = 4;
pub const OPT_LITE_SIGNATURE: u8 = 0x80;

/// The fixed key of the connection-signature HMAC (public: kinnay's NintendoClients wiki).
const CONNECTION_SIG_KEY: [u8; 15] = [
    0x26, 0xc3, 0x1f, 0x38, 0x1e, 0x46, 0xd6, 0xeb, 0x38, 0xe1, 0xaf, 0x6a, 0xb7, 0x0d, 0x11,
];

/// A server's connection signature for an IPv4 peer: HMAC-MD5 of its address and port.
pub fn connection_signature(addr: std::net::SocketAddrV4) -> [u8; 16] {
    let mut data = addr.ip().octets().to_vec();
    data.extend_from_slice(&addr.port().to_be_bytes());
    hmac_md5(&CONNECTION_SIG_KEY, &data)
}

/// A Lite CONNECT's proof that the client knows the game's access key:
/// HMAC-MD5(MD5(key), MD5(key) || the server's connection signature).
pub fn lite_connect_signature(access_key: &str, connection_sig: &[u8]) -> [u8; 16] {
    let key = md5(access_key.as_bytes());
    hmac_md5(&key, &[&key[..], connection_sig].concat())
}

pub mod lite {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Packet {
        pub kind: u8,
        pub flags: u16,
        pub src_type: u8,
        pub src_port: u8,
        pub dst_type: u8,
        pub dst_port: u8,
        pub fragment: u8,
        pub seq: u16,
        pub options: Vec<(u8, Vec<u8>)>,
        pub payload: Vec<u8>,
    }

    impl Packet {
        pub fn has(&self, flag: u16) -> bool {
            self.flags & flag != 0
        }

        pub fn option(&self, id: u8) -> Option<&[u8]> {
            self.options
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, v)| v.as_slice())
        }

        pub fn encode(&self) -> Vec<u8> {
            let opts: Vec<u8> = self
                .options
                .iter()
                .flat_map(|(id, v)| [&[*id, v.len() as u8][..], v].concat())
                .collect();
            let mut out = Vec::with_capacity(12 + opts.len() + self.payload.len());
            out.extend_from_slice(&[0x80, opts.len() as u8]);
            out.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
            out.extend_from_slice(&[
                self.src_type << 4 | self.dst_type & 0xF,
                self.src_port,
                self.dst_port,
                self.fragment,
            ]);
            out.extend_from_slice(&(self.kind as u16 & 0xF | self.flags << 4).to_le_bytes());
            out.extend_from_slice(&self.seq.to_le_bytes());
            out.extend(opts);
            out.extend_from_slice(&self.payload);
            out
        }
    }

    /// Every whole packet in `buf`; the bytes of an unfinished one are left in it.
    pub fn decode_all(buf: &mut Vec<u8>) -> Result<Vec<Packet>, Error> {
        let mut packets = Vec::new();
        let mut at = 0;
        while buf.len() - at >= 12 {
            if buf[at] != 0x80 {
                let first = buf[at];
                buf.clear();
                return Err(Error::Protocol(format!(
                    "not a PRUDP Lite packet ({first:#04x})"
                )));
            }
            let b = &buf[at..];
            let opt_len = b[1] as usize;
            let pay_len = u16::from_le_bytes([b[2], b[3]]) as usize;
            let total = 12 + opt_len + pay_len;
            if b.len() < total {
                break;
            }
            let tf = u16::from_le_bytes([b[8], b[9]]);
            let mut options = Vec::new();
            let mut o = &b[12..12 + opt_len];
            while o.len() >= 2 {
                let n = (o[1] as usize).min(o.len() - 2);
                options.push((o[0], o[2..2 + n].to_vec()));
                o = &o[2 + n..];
            }
            packets.push(Packet {
                kind: (tf & 0xF) as u8,
                flags: tf >> 4,
                src_type: b[4] >> 4,
                dst_type: b[4] & 0xF,
                src_port: b[5],
                dst_port: b[6],
                fragment: b[7],
                seq: u16::from_le_bytes([b[10], b[11]]),
                options,
                payload: b[12 + opt_len..total].to_vec(),
            });
            at += total;
        }
        buf.drain(..at);
        Ok(packets)
    }

    /// The support option: minor version | supported functions << 8.
    pub fn support(minor_version: u8, functions: u32) -> (u8, Vec<u8>) {
        (
            OPT_SUPPORT,
            (minor_version as u32 | functions << 8)
                .to_le_bytes()
                .to_vec(),
        )
    }
}

pub mod v1 {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Packet {
        pub kind: u8,
        pub flags: u16,
        pub src_type: u8,
        pub src_port: u8,
        pub dst_type: u8,
        pub dst_port: u8,
        pub session_id: u8,
        pub substream: u8,
        pub seq: u16,
        pub signature: [u8; 16],
        pub minor_version: u8,
        pub functions: u32,
        pub connection_sig: Vec<u8>,
        pub initial_unreliable_seq: u16,
        pub max_substream: u8,
        pub fragment: u8,
        pub payload: Vec<u8>,
    }

    impl Packet {
        /// The options this kind of packet carries.
        pub fn options(&self) -> Vec<u8> {
            let mut o = Vec::new();
            let mut put = |id: u8, v: &[u8]| {
                o.extend_from_slice(&[id, v.len() as u8]);
                o.extend_from_slice(v);
            };
            match self.kind {
                SYN | CONNECT => {
                    put(
                        OPT_SUPPORT,
                        &(self.minor_version as u32 | self.functions << 8).to_le_bytes(),
                    );
                    put(OPT_CONNECTION_SIG, &self.connection_sig);
                    if self.kind == CONNECT {
                        put(
                            OPT_UNRELIABLE_SEQ,
                            &self.initial_unreliable_seq.to_le_bytes(),
                        );
                    }
                    put(OPT_MAX_SUBSTREAM, &[self.max_substream]);
                }
                DATA => put(OPT_FRAGMENT_ID, &[self.fragment]),
                _ => {}
            }
            o
        }

        /// Version through sequence id (12 bytes; the signature covers bytes 4..12).
        fn header(&self, opt_len: usize) -> [u8; 12] {
            let mut h = [0u8; 12];
            h[0] = 1;
            h[1] = opt_len as u8;
            h[2..4].copy_from_slice(&(self.payload.len() as u16).to_le_bytes());
            h[4] = self.src_type << 4 | self.src_port & 0xF;
            h[5] = self.dst_type << 4 | self.dst_port & 0xF;
            h[6..8].copy_from_slice(&(self.kind as u16 & 0xF | self.flags << 4).to_le_bytes());
            h[8] = self.session_id;
            h[9] = self.substream;
            h[10..12].copy_from_slice(&self.seq.to_le_bytes());
            h
        }

        /// HMAC-MD5(MD5(key), header[4..] || session key || u32 sum of the key's bytes ||
        /// connection signature || options || payload).
        pub fn sign(
            &self,
            access_key: &str,
            session_key: &[u8],
            connection_sig: &[u8],
        ) -> [u8; 16] {
            let opts = self.options();
            let h = self.header(opts.len());
            let sum: u32 = access_key.bytes().map(u32::from).sum();
            let data = [
                &h[4..],
                session_key,
                &sum.to_le_bytes(),
                connection_sig,
                &opts,
                &self.payload,
            ]
            .concat();
            hmac_md5(&md5(access_key.as_bytes()), &data)
        }

        pub fn encode(&self) -> Vec<u8> {
            let opts = self.options();
            let mut out = vec![0xEA, 0xD0];
            out.extend_from_slice(&self.header(opts.len()));
            out.extend_from_slice(&self.signature);
            out.extend(opts);
            out.extend_from_slice(&self.payload);
            out
        }

        pub fn decode(b: &[u8]) -> Result<Self, Error> {
            if b.len() < 30 || b[..2] != [0xEA, 0xD0] || b[2] != 1 {
                return Err(Error::Protocol("not a PRUDP V1 packet".into()));
            }
            let opt_len = b[3] as usize;
            let pay_len = u16::from_le_bytes([b[4], b[5]]) as usize;
            if b.len() < 30 + opt_len + pay_len {
                return Err(Error::Truncated);
            }
            let tf = u16::from_le_bytes([b[8], b[9]]);
            let mut p = Packet {
                kind: (tf & 0xF) as u8,
                flags: tf >> 4,
                src_type: b[6] >> 4,
                src_port: b[6] & 0xF,
                dst_type: b[7] >> 4,
                dst_port: b[7] & 0xF,
                session_id: b[10],
                substream: b[11],
                seq: u16::from_le_bytes([b[12], b[13]]),
                signature: b[14..30].try_into().unwrap(),
                payload: b[30 + opt_len..30 + opt_len + pay_len].to_vec(),
                ..Default::default()
            };
            let mut o = &b[30..30 + opt_len];
            while o.len() >= 2 {
                let n = (o[1] as usize).min(o.len() - 2);
                let v = &o[2..2 + n];
                match (o[0], n) {
                    (OPT_SUPPORT, 4) => {
                        let x = u32::from_le_bytes(v.try_into().unwrap());
                        p.minor_version = x as u8;
                        p.functions = x >> 8;
                    }
                    (OPT_CONNECTION_SIG, _) => p.connection_sig = v.to_vec(),
                    (OPT_UNRELIABLE_SEQ, 2) => {
                        p.initial_unreliable_seq = u16::from_le_bytes([v[0], v[1]])
                    }
                    (OPT_MAX_SUBSTREAM, 1) => p.max_substream = v[0],
                    (OPT_FRAGMENT_ID, 1) => p.fragment = v[0],
                    _ => {}
                }
                o = &o[2 + n..];
            }
            Ok(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::hex;

    #[test]
    fn lite_round_trip_and_partial_buffers() {
        let p = lite::Packet {
            kind: CONNECT,
            flags: FLAG_RELIABLE | FLAG_NEED_ACK,
            src_type: STREAM_RV_SECURE,
            src_port: 31,
            dst_type: STREAM_RV_SECURE,
            dst_port: 1,
            seq: 1,
            options: vec![lite::support(6, 0), (OPT_LITE_SIGNATURE, vec![7; 16])],
            payload: vec![1, 2, 3],
            ..Default::default()
        };
        let b = p.encode();
        assert_eq!(&b[..12], [0x80, 24, 3, 0, 0xAA, 31, 1, 0, 0x61, 0, 1, 0]);
        let mut buf = [b.clone(), b[..5].to_vec()].concat();
        let got = lite::decode_all(&mut buf).unwrap();
        assert_eq!(got, [p]);
        assert_eq!(buf, b[..5]);
        assert!(lite::decode_all(&mut vec![0x81; 12]).is_err());
    }

    #[test]
    fn v1_round_trip() {
        let mut p = v1::Packet {
            kind: SYN,
            flags: FLAG_NEED_ACK,
            src_type: 10,
            src_port: 15,
            dst_type: 10,
            dst_port: 1,
            minor_version: 2,
            functions: 4,
            connection_sig: vec![0; 16],
            ..Default::default()
        };
        p.signature = p.sign("abcd1234", &[], &[]);
        let q = v1::Packet::decode(&p.encode()).unwrap();
        assert_eq!(q, p);
        assert_eq!(q.sign("abcd1234", &[], &[]), p.signature);
    }

    // Known answers computed with nextendo-nex's Go implementation (ConnectionSignature,
    // LitePacketSignature, EncodePacket, EncodeUDPPacket, UDPPacketSignature) for the same inputs.
    #[test]
    fn signatures_and_encodings_match_the_server() {
        let sig = connection_signature("127.0.0.1:50000".parse().unwrap());
        assert_eq!(hex(&sig), "2b3f15ba1a782050c30390438b224637");
        assert_eq!(
            hex(&lite_connect_signature("testkey0", &sig)),
            "95d121808aaa90b17cfd6708df056960"
        );
        let syn_ack = lite::Packet {
            kind: SYN,
            flags: FLAG_ACK | FLAG_HAS_SIZE,
            src_type: 10,
            src_port: 1,
            dst_type: 10,
            dst_port: 31,
            options: vec![lite::support(6, 0), (OPT_CONNECTION_SIG, sig.to_vec())],
            ..Default::default()
        };
        assert_eq!(
            hex(&syn_ack.encode()),
            "80180000aa011f00900000000004060000000110\
             2b3f15ba1a782050c30390438b224637"
        );
        let p = v1::Packet {
            kind: SYN,
            flags: FLAG_NEED_ACK,
            src_type: 10,
            src_port: 15,
            dst_type: 10,
            dst_port: 1,
            minor_version: 2,
            functions: 4,
            connection_sig: vec![0; 16],
            payload: vec![1, 2, 3],
            ..Default::default()
        };
        assert_eq!(
            hex(&p.encode()),
            "ead0011b0300afa140000000000000000000000000000000000000000000\
             000402040000011000000000000000000000000000000000040100010203"
        );
        assert_eq!(
            hex(&p.sign("testkey0", b"sessionkey", b"connsig")),
            "cdcc1cbe0a8fe2b9f5d7e0f5d805f462"
        );
    }
}

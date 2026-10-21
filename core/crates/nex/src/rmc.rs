//! RMC, NEX's remote method calls. On the wire, after a u32 length:
//! - the protocol id (bit 0x80 set on a request; 0x7F means a u16 protocol id follows);
//! - a request: u32 call id, u32 method id, the parameters;
//! - a response: bool success, then u32 call id, u32 method id | 0x8000 and the result, or
//!   on failure u32 error code and u32 call id.

use crate::Error;
use crate::stream::{Reader, Writer};

/// The result code of a successful call ("Core::Unknown" without the error bit).
pub const RESULT_SUCCESS: u32 = 0x0001_0001;
/// The error bit of a result code.
pub const ERROR_BIT: u32 = 0x8000_0000;
pub const CORE_NOT_IMPLEMENTED: u32 = 0x8001_0002;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Request {
        protocol: u16,
        call_id: u32,
        method: u32,
        body: Vec<u8>,
    },
    Response {
        protocol: u16,
        call_id: u32,
        /// The method and its result, or the error code.
        result: Result<(u32, Vec<u8>), u32>,
    },
}

fn protocol_bytes(w: &mut Writer, protocol: u16, request: bool) {
    let flag = if request { 0x80 } else { 0 };
    if protocol < 0x7F {
        w.u8(protocol as u8 | flag);
    } else {
        w.u8(0x7F | flag).u16(protocol);
    }
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Message::Request {
                protocol,
                call_id,
                method,
                body,
            } => {
                protocol_bytes(&mut w, *protocol, true);
                w.u32(*call_id).u32(*method).raw(body);
            }
            Message::Response {
                protocol,
                call_id,
                result: Ok((method, body)),
            } => {
                protocol_bytes(&mut w, *protocol, false);
                w.bool(true).u32(*call_id).u32(method | 0x8000).raw(body);
            }
            Message::Response {
                protocol,
                call_id,
                result: Err(code),
            } => {
                protocol_bytes(&mut w, *protocol, false);
                w.bool(false).u32(code | ERROR_BIT).u32(*call_id);
            }
        }
        let body = w.into_bytes();
        let mut out = (body.len() as u32).to_le_bytes().to_vec();
        out.extend(body);
        out
    }

    pub fn decode(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        if r.u32()? as usize != r.remaining() {
            return Err(Error::Protocol("RMC length does not match".into()));
        }
        let first = r.u8()?;
        let mut protocol = (first & 0x7F) as u16;
        if protocol == 0x7F {
            protocol = r.u16()?;
        }
        Ok(if first & 0x80 != 0 {
            Message::Request {
                protocol,
                call_id: r.u32()?,
                method: r.u32()?,
                body: r.rest().to_vec(),
            }
        } else if r.bool()? {
            let call_id = r.u32()?;
            let method = r.u32()? & !0x8000;
            Message::Response {
                protocol,
                call_id,
                result: Ok((method, r.rest().to_vec())),
            }
        } else {
            let code = r.u32()?;
            Message::Response {
                protocol,
                call_id: r.u32()?,
                result: Err(code),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_responses_round_trip() {
        let req = Message::Request {
            protocol: 0x0A,
            call_id: 7,
            method: 6,
            body: vec![1, 2, 3],
        };
        let b = req.encode();
        assert_eq!(&b[..5], [12, 0, 0, 0, 0x8A]);
        assert_eq!(Message::decode(&b).unwrap(), req);

        let big = Message::Request {
            protocol: 0x6D,
            call_id: 1,
            method: 40,
            body: vec![],
        };
        assert_eq!(Message::decode(&big.encode()).unwrap(), big);
        let ext = Message::Response {
            protocol: 0x80,
            call_id: 2,
            result: Ok((3, vec![9])),
        };
        let b = ext.encode();
        assert_eq!(&b[4..7], [0x7F, 0x80, 0x00]);
        assert_eq!(Message::decode(&b).unwrap(), ext);

        let err = Message::Response {
            protocol: 0x0B,
            call_id: 4,
            result: Err(CORE_NOT_IMPLEMENTED),
        };
        assert_eq!(Message::decode(&err.encode()).unwrap(), err);
        assert!(Message::decode(&[9, 0, 0, 0, 1]).is_err());
    }
}

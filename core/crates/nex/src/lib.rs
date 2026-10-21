//! A NEX client, as Nextendo Network's community game servers speak it (G6: Network Play).
//!
//! Nextendo provides the sign-in, matchmaking and NAT traversal; the game between SMCraft
//! players is SMCraft's own rollback netcode (`mmk-netplay`) peer to peer. SMCraft's sessions
//! use their own game mode and attributes, so they never mix with real consoles' sessions.
//!
//! The layers, bottom up (all written from the public protocol documentation, kinnay's
//! NintendoClients wiki, and checked byte for byte against nextendo-nex, the server):
//! - [`ws`]: a WebSocket client (the Switch carries NEX over WebSocket, behind TLS on :443);
//! - [`prudp`]: PRUDP Lite packets (and the older V1 codec);
//! - [`conn`]: a PRUDP Lite connection: handshake, reliable ordered delivery, fragments, acks,
//!   pings;
//! - [`rmc`] and [`stream`]: remote method calls and NEX's types;
//! - [`kerberos`]: the tickets;
//! - [`client`]: the login (TicketGranting), the secure connection (SecureConnection.Register)
//!   and the calls on it (MatchmakeExtension, MatchMaking, NATTraversal), and the requests the
//!   server sends (notifications, probe requests);
//! - [`matchmaking`]: gatherings, sessions, search criteria, notifications;
//! - [`token`]: the ID token that carries Nextendo's game token, and test tokens;
//! - [`nat`]: the NAT check and SMCraft's UDP hole punch;
//! - [`play`]: the whole Network Play flow, ending in peer addresses for `mmk-netplay`.
//!
//! It never connects anywhere by itself: the caller names the server.

pub mod client;
pub mod conn;
pub mod crypto;
pub mod kerberos;
pub mod matchmaking;
pub mod nat;
pub mod play;
pub mod prudp;
pub mod rmc;
pub mod station;
pub mod stream;
pub mod token;
pub mod ws;

use std::fmt;

pub use client::{GameServer, Login, Secure};
pub use station::StationUrl;

#[derive(Debug)]
pub enum Error {
    /// The data ended before the value did.
    Truncated,
    Io(std::io::Error),
    /// The other side broke the protocol (or isn't a NEX server).
    Protocol(String),
    /// A call answered with an error code.
    Rmc {
        protocol: u16,
        method: u32,
        code: u32,
    },
    /// No answer in time.
    Timeout(&'static str),
    /// The connection was closed.
    Closed,
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "truncated data"),
            Error::Io(e) => write!(f, "{e}"),
            Error::Protocol(s) => write!(f, "protocol error: {s}"),
            Error::Rmc {
                protocol,
                method,
                code,
            } => write!(
                f,
                "call {protocol:#x}.{method} failed with {code:#010x}{}",
                result_name(*code)
                    .map(|n| format!(" ({n})"))
                    .unwrap_or_default()
            ),
            Error::Timeout(what) => write!(f, "no answer in time ({what})"),
            Error::Closed => write!(f, "the server closed the connection"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                Error::Timeout("socket")
            }
            _ => Error::Io(e),
        }
    }
}

/// Names of the result codes a player might see.
pub fn result_name(code: u32) -> Option<&'static str> {
    Some(match code & !rmc::ERROR_BIT {
        0x0001_0002 => "Core::NotImplemented",
        0x0001_0006 => "Core::AccessDenied",
        0x0001_000A => "Core::InvalidArgument",
        0x0003_006B => "RendezVous::InvalidPID",
        0x0003_0073 => "RendezVous::SessionVoid",
        0x0003_00C8 => "RendezVous::SessionFull",
        0x0003_00D9 => "RendezVous::PermissionDenied",
        0x0068_0002 => "Authentication::TokenParseError",
        0x0068_000C => "Authentication::UnsupportedVersion",
        0x0068_000F => "Authentication::ClientVersionIsOld",
        _ => return None,
    })
}

//! The NEX login and the secure connection.
//!
//! 1. **Login** ([`login`]): a PRUDP connection to the authentication server (port 31 → 1) and
//!    TicketGranting.ValidateAndRequestTicketWithParam (0x0A / 6): platform 3, the PID as the
//!    user name, an `AuthenticationInfo` holding the ID token, the NEX and client versions.
//!    The answer: our PID, the ticket (opened with the source key that comes with it), the
//!    secure server's station URL and the server time. The connection is closed after.
//! 2. **Secure** ([`Secure::connect`]): a PRUDP connection to the secure server (30 → 2)
//!    whose CONNECT carries the ticket's inner blob and, encrypted with the session key, our
//!    PID, the connection id and a check value; the server proves it read them by answering
//!    the check value + 1. Then SecureConnection.Register (0x0B / 1) with our station URLs,
//!    and the calls of matchmaking and NAT traversal.
//!
//! The server also sends requests on the secure connection: notifications (0x0E / 1) and
//! NAT traversal's InitiateProbe (0x03 / 2). They are answered at once and queued as
//! [`Event`]s.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::conn::Conn;
use crate::crypto::{random_bytes, unhex};
use crate::kerberos::{self, ClientTicket};
use crate::matchmaking::*;
use crate::rmc::Message;
use crate::station::StationUrl;
use crate::stream::{DateTime, Reader, Structure, Writer};
use crate::ws::{self, Transport, WebSocket};
use crate::{Error, result_name};

pub const PROTOCOL_TICKET_GRANTING: u16 = 0x0A;
pub const PROTOCOL_SECURE_CONNECTION: u16 = 0x0B;
pub const PROTOCOL_NAT_TRAVERSAL: u16 = 0x03;

/// SMM2's NEX identity (from the game's code; public in kinnay's game server list), for when
/// SMCraft logs in to Nextendo's SMM2 game server. The access key is not kept here: it comes
/// from the player's own copy (configuration), like every other value taken from the game.
pub mod smm2 {
    pub const GAME_SERVER_ID: u32 = 0x2230_6D00;
    /// The game's host name for its NEX server (`%` → `lp1`); Nextendo routes it.
    pub const HOST: &str = "g22306d00-lp1.s.n.srv.nintendo.net";
    pub const NEX_VERSION: u32 = 40605;
    pub const CLIENT_VERSION: u32 = 60;
}

/// The platform type in the login (the Switch).
pub const PLATFORM_SWITCH: u32 = 3;

/// Opens a TLS transport to (host, port), for `wss://` servers.
pub type Connector =
    Arc<dyn Fn(&str, u16, Duration) -> Result<Box<dyn Transport>, Error> + Send + Sync>;

/// Where and how to log in.
#[derive(Clone)]
pub struct GameServer {
    /// The authentication server, `ws://host:port/` (a local server) or `wss://host/`.
    pub auth_url: String,
    pub access_key: String,
    pub nex_version: u32,
    pub client_version: u32,
    pub timeout: Duration,
    /// Makes the TLS connection for `wss://`.
    pub tls: Option<Connector>,
}

impl GameServer {
    /// SMM2's versions against the server at `auth_url`.
    pub fn smm2(auth_url: &str, access_key: &str) -> Self {
        Self {
            auth_url: auth_url.into(),
            access_key: access_key.into(),
            nex_version: smm2::NEX_VERSION,
            client_version: smm2::CLIENT_VERSION,
            timeout: Duration::from_secs(10),
            tls: None,
        }
    }

    fn open(&self, url: &str) -> Result<WebSocket, Error> {
        let (tls, host, port, path) = ws::parse_url(url)?;
        if !tls {
            return WebSocket::connect(url, self.timeout);
        }
        let make = self
            .tls
            .as_ref()
            .ok_or_else(|| Error::Protocol("wss:// without a TLS connector".into()))?;
        let io = make(&host, port, self.timeout)?;
        WebSocket::over(io, &host, &path, self.timeout)
    }
}

/// A request the server sent.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Notification(NotificationEvent),
    /// NAT traversal: a peer asks to be probed at this station.
    InitiateProbe(StationUrl),
    Other {
        protocol: u16,
        method: u32,
        body: Vec<u8>,
    },
}

/// RMC calls over one PRUDP connection.
struct Calls {
    conn: Conn,
    next: u32,
    events: VecDeque<Event>,
}

impl Calls {
    fn new(conn: Conn) -> Self {
        Self {
            conn,
            next: 1,
            events: VecDeque::new(),
        }
    }

    fn call(
        &mut self,
        protocol: u16,
        method: u32,
        body: Vec<u8>,
        timeout: Duration,
    ) -> Result<Vec<u8>, Error> {
        let call_id = self.next;
        self.next = self.next.wrapping_add(1);
        self.conn.send(
            &Message::Request {
                protocol,
                call_id,
                method,
                body,
            }
            .encode(),
        )?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout("RMC call"));
            }
            let Some(m) = self.conn.recv(left)? else {
                continue;
            };
            match Message::decode(&m)? {
                Message::Response {
                    call_id: id,
                    result,
                    ..
                } if id == call_id => {
                    return result.map(|(_, b)| b).map_err(|code| Error::Rmc {
                        protocol,
                        method,
                        code,
                    });
                }
                Message::Request {
                    protocol,
                    call_id,
                    method,
                    body,
                } => self.answer(protocol, call_id, method, body)?,
                _ => {}
            }
        }
    }

    /// Answers a request from the server (success, nothing more) and queues it.
    fn answer(
        &mut self,
        protocol: u16,
        call_id: u32,
        method: u32,
        body: Vec<u8>,
    ) -> Result<(), Error> {
        self.conn.send(
            &Message::Response {
                protocol,
                call_id,
                result: Ok((method, Vec::new())),
            }
            .encode(),
        )?;
        let event = match (protocol, method) {
            (PROTOCOL_NOTIFICATIONS, 1) => Reader::new(&body)
                .structure::<NotificationEvent>()
                .map(Event::Notification)
                .ok(),
            (PROTOCOL_NAT_TRAVERSAL, 2) => {
                Reader::new(&body).station().map(Event::InitiateProbe).ok()
            }
            _ => None,
        };
        self.events.push_back(event.unwrap_or(Event::Other {
            protocol,
            method,
            body,
        }));
        Ok(())
    }

    fn poll(&mut self, timeout: Duration) -> Result<Vec<Event>, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.conn.recv(left)? {
                Some(m) => {
                    if let Message::Request {
                        protocol,
                        call_id,
                        method,
                        body,
                    } = Message::decode(&m)?
                    {
                        self.answer(protocol, call_id, method, body)?;
                    }
                }
                None => break,
            }
            if !self.events.is_empty() && Instant::now() >= deadline {
                break;
            }
        }
        Ok(self.events.drain(..).collect())
    }
}

/// AuthenticationInfo (a Data subclass): the ID token and what kind it is.
#[derive(Default)]
struct AuthenticationInfo {
    token: String,
}

impl Structure for AuthenticationInfo {
    const VERSIONS: &'static [u8] = &[0, 0];
    fn save(&self, level: usize, w: &mut Writer) {
        if level == 1 {
            // NGS version 4, token type 2 (an ID token), server version 0
            w.string(&self.token).u32(4).u8(2).u32(0);
        }
    }
    fn load(&mut self, level: usize, r: &mut Reader) -> Result<(), Error> {
        if level == 1 {
            self.token = r.string()?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct TicketResult {
    pid: u64,
    ticket: Vec<u8>,
    station: StationUrl,
    time: DateTime,
    name: String,
    source_key: String,
}

impl Structure for TicketResult {
    const VERSIONS: &'static [u8] = &[1];
    fn save(&self, _: usize, w: &mut Writer) {
        w.pid(self.pid)
            .buffer(&self.ticket)
            .station(&self.station)
            .datetime(self.time)
            .string(&self.name)
            .string(&self.source_key);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.pid = r.pid()?;
        self.ticket = r.buffer()?.to_vec();
        self.station = r.station()?;
        self.time = r.datetime()?;
        self.name = r.string()?;
        self.source_key = r.string()?;
        Ok(())
    }
}

/// What the login gave.
#[derive(Clone, Debug)]
pub struct Login {
    pub pid: u64,
    pub ticket: ClientTicket,
    pub secure_station: StationUrl,
    pub server_name: String,
    pub server_time: DateTime,
}

/// Logs in as `pid` with `id_token` (see [`crate::token::id_token`]).
pub fn login(server: &GameServer, pid: u64, id_token: &str) -> Result<Login, Error> {
    let ws = server.open(&server.auth_url)?;
    let (conn, _) = Conn::connect(ws, &server.access_key, 31, 1, &[], server.timeout)?;
    let mut calls = Calls::new(conn);
    let mut body = Writer::new();
    let mut param = Writer::new();
    param
        .u32(PLATFORM_SWITCH)
        .string(&pid.to_string())
        .any_data(
            "AuthenticationInfo",
            &AuthenticationInfo {
                token: id_token.into(),
            },
        )
        .bool(false)
        .u32(server.nex_version)
        .u32(server.client_version);
    body.u8(0).buffer(param.bytes());
    let answer = calls.call(
        PROTOCOL_TICKET_GRANTING,
        6,
        body.into_bytes(),
        server.timeout,
    );
    calls.conn.disconnect();
    let r: TicketResult = Reader::new(&answer?).structure()?;
    let key = unhex(&r.source_key)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Protocol("the login answer has no source key".into()))?;
    let ticket = ClientTicket::decrypt(&key, &r.ticket)?;
    if r.pid != pid {
        return Err(Error::Protocol("the login answered for another PID".into()));
    }
    Ok(Login {
        pid: r.pid,
        ticket,
        secure_station: r.station,
        server_name: r.name,
        server_time: r.time,
    })
}

/// The secure connection, where matchmaking happens.
pub struct Secure {
    calls: Calls,
    pub pid: u64,
    /// The connection id the server gave at Register (RVCID).
    pub rvcid: u32,
    pub timeout: Duration,
}

impl Secure {
    /// Connects to the secure server named by the login.
    pub fn connect(server: &GameServer, login: &Login) -> Result<Self, Error> {
        let st = &login.secure_station;
        let url = match st.get("address") {
            // "the same server"
            Some("0.0.0.1") | None => server.auth_url.clone(),
            Some(a) => {
                let scheme = if server.auth_url.starts_with("wss://") {
                    "wss"
                } else {
                    "ws"
                };
                format!("{scheme}://{a}:{}/", st.get("port").unwrap_or("443"))
            }
        };
        let ws = server.open(&url)?;
        let check = u32::from_le_bytes(random_bytes(4).try_into().unwrap());
        let cid = st.get_u64("CID").unwrap_or(0) as u32;
        let mut req = Writer::new();
        req.pid(login.pid).u32(cid).u32(check);
        let mut payload = Writer::new();
        payload
            .buffer(&login.ticket.internal)
            .buffer(&kerberos::encrypt(&login.ticket.session_key, req.bytes()));
        let dst = st.get_u64("sid").unwrap_or(1) as u8;
        let (conn, answer) = Conn::connect(
            ws,
            &server.access_key,
            30,
            dst,
            payload.bytes(),
            server.timeout,
        )?;
        let mut r = Reader::new(&answer);
        let proof = Reader::new(r.buffer()?).u32()?;
        if proof != check.wrapping_add(1) {
            return Err(Error::Protocol(
                "the secure server did not prove the session key".into(),
            ));
        }
        Ok(Self {
            calls: Calls::new(conn),
            pid: login.pid,
            rvcid: 0,
            timeout: server.timeout,
        })
    }

    pub fn call(&mut self, protocol: u16, method: u32, body: Vec<u8>) -> Result<Vec<u8>, Error> {
        self.calls.call(protocol, method, body, self.timeout)
    }

    /// Requests from the server that arrived (waiting at most `timeout` for the first); also
    /// keeps the connection alive.
    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<Event>, Error> {
        self.calls.poll(timeout)
    }

    /// SecureConnection.Register: our station URLs → the connection id and the public URL
    /// the server sees.
    pub fn register(&mut self, urls: &[StationUrl]) -> Result<StationUrl, Error> {
        let mut w = Writer::new();
        w.list(urls, |w, u| {
            w.station(u);
        });
        let b = self.call(PROTOCOL_SECURE_CONNECTION, 1, w.into_bytes())?;
        let mut r = Reader::new(&b);
        let result = r.u32()?;
        if result & crate::rmc::ERROR_BIT != 0 {
            return Err(Error::Rmc {
                protocol: PROTOCOL_SECURE_CONNECTION,
                method: 1,
                code: result,
            });
        }
        self.rvcid = r.u32()?;
        r.station()
    }

    /// SecureConnection.ReplaceURL.
    pub fn replace_url(&mut self, old: &StationUrl, new: &StationUrl) -> Result<(), Error> {
        let mut w = Writer::new();
        w.station(old).station(new);
        self.call(PROTOCOL_SECURE_CONNECTION, 7, w.into_bytes())
            .map(|_| ())
    }

    fn session_call(
        &mut self,
        method: u32,
        param: impl FnOnce(&mut Writer),
    ) -> Result<MatchmakeSession, Error> {
        let mut w = Writer::new();
        param(&mut w);
        let b = self.call(PROTOCOL_MATCHMAKE_EXTENSION, method, w.into_bytes())?;
        Reader::new(&b).structure()
    }

    /// MatchmakeExtension.AutoMatchmakeWithParam_Postpone: join a matching session or open one.
    pub fn auto_matchmake(&mut self, p: &AutoMatchmakeParam) -> Result<MatchmakeSession, Error> {
        self.session_call(40, |w| {
            w.structure(p);
        })
    }

    /// MatchmakeExtension.CreateMatchmakeSessionWithParam.
    pub fn create_session(
        &mut self,
        p: &CreateMatchmakeSessionParam,
    ) -> Result<MatchmakeSession, Error> {
        self.session_call(38, |w| {
            w.structure(p);
        })
    }

    /// MatchmakeExtension.JoinMatchmakeSessionWithParam.
    pub fn join_session(
        &mut self,
        p: &JoinMatchmakeSessionParam,
    ) -> Result<MatchmakeSession, Error> {
        self.session_call(39, |w| {
            w.structure(p);
        })
    }

    /// MatchmakeExtension.FindMatchmakeSessionBySingleGatheringId.
    pub fn find_session(&mut self, gid: u32) -> Result<MatchmakeSession, Error> {
        self.session_call(0x31, |w| {
            w.u32(gid);
        })
    }

    /// MatchmakeExtension.BrowseMatchmakeSessionNoHolderNoResultRange: the open sessions that
    /// pass `criteria` (the server may filter less; check [`MatchmakeSession::is_smcraft`]).
    pub fn browse(&mut self, criteria: &SearchCriteria) -> Result<Vec<MatchmakeSession>, Error> {
        let mut w = Writer::new();
        w.structure(criteria);
        let b = self.call(PROTOCOL_MATCHMAKE_EXTENSION, 0x34, w.into_bytes())?;
        Reader::new(&b).list(|r| r.structure())
    }

    fn gid_call(&mut self, protocol: u16, method: u32, gid: u32) -> Result<Vec<u8>, Error> {
        let mut w = Writer::new();
        w.u32(gid);
        self.call(protocol, method, w.into_bytes())
    }

    /// MatchmakeExtension.CloseParticipation / OpenParticipation.
    pub fn set_participation(&mut self, gid: u32, open: bool) -> Result<(), Error> {
        self.gid_call(PROTOCOL_MATCHMAKE_EXTENSION, if open { 2 } else { 1 }, gid)
            .map(|_| ())
    }

    /// MatchMaking.GetSessionURLs: the host's station URLs.
    pub fn session_urls(&mut self, gid: u32) -> Result<Vec<StationUrl>, Error> {
        let b = self.gid_call(PROTOCOL_MATCHMAKING, 41, gid)?;
        Reader::new(&b).list(|r| r.station())
    }

    /// MatchMaking.GetDetailedParticipants.
    pub fn participants(&mut self, gid: u32) -> Result<Vec<ParticipantDetails>, Error> {
        let b = self.gid_call(PROTOCOL_MATCHMAKING, 15, gid)?;
        Reader::new(&b).list(|r| r.structure())
    }

    /// MatchMaking.UnregisterGathering (the owner only).
    pub fn unregister_gathering(&mut self, gid: u32) -> Result<bool, Error> {
        let b = self.gid_call(PROTOCOL_MATCHMAKING, 2, gid)?;
        Reader::new(&b).bool()
    }

    /// MatchMakingExt.EndParticipation: leave a gathering.
    pub fn end_participation(&mut self, gid: u32, message: &str) -> Result<bool, Error> {
        let mut w = Writer::new();
        w.u32(gid).string(message);
        let b = self.call(PROTOCOL_MATCHMAKING_EXT, 1, w.into_bytes())?;
        Reader::new(&b).bool()
    }

    /// NATTraversal.RequestProbeInitiationExt: the server asks each target (a station URL with
    /// its RVCID) to probe `me` — so both ends punch at the same time.
    pub fn request_probe_initiation(
        &mut self,
        targets: &[StationUrl],
        me: &StationUrl,
    ) -> Result<(), Error> {
        let mut w = Writer::new();
        w.list(targets, |w, t| {
            w.string(&t.to_string());
        })
        .string(&me.to_string());
        self.call(PROTOCOL_NAT_TRAVERSAL, 3, w.into_bytes())
            .map(|_| ())
    }

    /// NATTraversal.ReportNATProperties (mapping, filtering, round trip ms).
    pub fn report_nat_properties(
        &mut self,
        mapping: u32,
        filtering: u32,
        rtt: u32,
    ) -> Result<(), Error> {
        let mut w = Writer::new();
        w.u32(mapping).u32(filtering).u32(rtt);
        self.call(PROTOCOL_NAT_TRAVERSAL, 5, w.into_bytes())
            .map(|_| ())
    }

    /// NATTraversal.ReportNATTraversalResult (the peer's connection id, worked, round trip).
    pub fn report_nat_result(&mut self, cid: u32, ok: bool, rtt: u32) -> Result<(), Error> {
        let mut w = Writer::new();
        w.u32(cid).bool(ok).u32(rtt);
        self.call(PROTOCOL_NAT_TRAVERSAL, 4, w.into_bytes())
            .map(|_| ())
    }

    pub fn disconnect(self) {
        self.calls.conn.disconnect();
    }
}

/// A login error in words for the player.
pub fn describe(e: &Error) -> String {
    match e {
        Error::Rmc { code, .. } => match result_name(*code) {
            Some("Authentication::TokenParseError") => {
                "the server refused the sign-in (sign in to Nextendo again)".into()
            }
            _ => e.to_string(),
        },
        _ => e.to_string(),
    }
}

# Network Play through Nextendo Network

SMCraft's Network Play uses [Nextendo Network](https://nextendo.network), the community's server
for Switch games, for three things only: **sign-in, matchmaking and NAT traversal**. The game
itself runs between SMCraft players on SMCraft's own rollback netcode (`mmk-netplay`), peer to
peer. There is no play against real consoles, and SMCraft's sessions never meet a console's
(their own game mode, attributes and application data, below).

The client is `core/crates/nex` (`mmk-nex`), written from the public NEX documentation
(kinnay's NintendoClients wiki) and checked against Nextendo's public server core
(`nextendo-nex`, Go) running on this machine. None of Nextendo's code is in this repository; their
code is source-available under PolyForm Shield and is only fetched to build the local test
server. Nothing here has connected to Nextendo's live game servers yet.

## The layers

| Layer | Where | What |
|---|---|---|
| Sign-in | `mmk-online::nextendo` | OAuth 2.0 + PKCE in the player's browser, the game token |
| Transport | `mmk-nex::ws`, `::tls` | WebSocket (`NEX` subprotocol) over TLS on :443, the server's key pinned |
| PRUDP | `mmk-nex::prudp`, `::conn` | PRUDP "Lite" (the Switch's), one packet per WebSocket frame |
| RMC | `mmk-nex::rmc`, `::stream` | remote method calls and NEX's types |
| Login | `mmk-nex::client`, `::kerberos`, `::token` | TicketGranting, the ticket, the secure connection |
| Matchmaking | `mmk-nex::client`, `::matchmaking` | MatchmakeExtension, MatchMaking, MatchMakingExt, notifications |
| NAT traversal | `mmk-nex::nat`, `::play` | NAT check, probe requests, SMCraft's UDP hole punch |
| The game | `mmk-netplay` | rollback over the punched UDP socket |

## 1. Sign-in (the account server)

Nextendo's developer docs tell a third-party app to do this; `mmk-online::nextendo` does:

1. Open a listener on `127.0.0.1` (any free port) and send the player's browser to
   `https://nextendo.network/oauth/authorize?response_type=code&client_id=…&redirect_uri=http://127.0.0.1:<port>/callback&scope=identity%20game.matchmaking&state=…&code_challenge=…&code_challenge_method=S256`.
2. The browser comes back to `/callback?code=…&state=…`. A wrong `state` is refused (a forged
   callback), `error=access_denied` means the player said no.
3. `POST /oauth/token` (or `/api/oauth/token`), form-encoded: `grant_type=authorization_code`,
   `client_id`, `code`, `redirect_uri`, `code_verifier` → `access_token` (1 h), maybe a
   `refresh_token` (rotated on every refresh: the new one is stored before the access token is
   used).
4. `GET /api/nex-token` with `Authorization: Bearer <access token>` → `nex_token` (the game
   token, 24 h), `pid`, `username`, `friend_code`.

The password never passes through SMCraft. Tokens go only to the host's secret store (the
Keychain on Apple; `mmk-play` keeps a private file, mode 0600, in the player data folder). The
PID works as a credential: SMCraft never shows or logs it, nor a peer's address.

The game token is `nx2.` + base64url(`pid.username.expiry`) + `.` + base64url(HMAC-SHA256 by the
account server). It reaches the game server inside an **ID token**: a JWT whose `nnex` claim is
the game token and `tv` the game version (`3.0.3`). The game server checks the game token (its
signature, expiry and PID), not the JWT's own signature; SMCraft signs its JWT with a key of its
own.

The client ID is the application registered with Nextendo. By default `mmk-online` uses the
owner's "SMM2Recomp" registration (`nxc_R-JWtJ6JrrgY`, a public client: no secret);
`SMCRAFT_NEXTENDO_CLIENT_ID` replaces it.

## 2. Transport: WebSocket, TLS, PRUDP Lite

The Switch reaches its NEX server over TLS on port 443, upgrades to WebSocket (`GET /`,
`Sec-WebSocket-Protocol: NEX`, ALPN `http/1.1`) and sends one PRUDP packet per binary frame.
Nextendo's game servers present a self-signed certificate, so SMCraft **pins** its key (the
SHA-256 of the certificate's SubjectPublicKeyInfo); a different key fails the handshake.

PRUDP Lite header (12 bytes, little-endian), then options and payload:

```text
u8 0x80 | u8 option length | u16 payload length | u8 src type << 4 | dst type
u8 src port | u8 dst port | u8 fragment id | u16 type | flags << 4 | u16 sequence id
```

Types SYN 0, CONNECT 1, DATA 2, DISCONNECT 3, PING 4. Flags ACK 1, RELIABLE 2, NEED_ACK 4,
HAS_SIZE 8, MULTI_ACK 0x200. Options `id, length, value`: 0 support (u32: minor version |
functions << 8), 1 the server's connection signature (16 bytes), 0x80 the client's Lite signature.

A connection, as SMM2 3.0.3 runs it (both stream types 10, "RVSecure"):

1. SYN (NEED_ACK, sequence 0, support `6`) → SYN-ACK with the connection signature.
2. CONNECT (RELIABLE | NEED_ACK, sequence 1, support, and the Lite signature
   HMAC-MD5(MD5(access key), MD5(access key) ‖ connection signature), the payload) →
   CONNECT-ACK with the server's answer. A wrong access key gets no answer.
3. DATA, RELIABLE | NEED_ACK, sequence ids from 2. A message is cut into fragments of 1300 bytes
   numbered 1, 2, …, the last 0 (a single fragment is 0). Every reliable packet is acknowledged
   (DATA | ACK with its sequence and fragment id) and delivered once, in sequence order; the
   server's own sequence starts at 1.
4. PING (NEED_ACK) when idle (the game: about every 10 s; SMCraft: 5 s), DISCONNECT at the end.

The authentication connection uses ports 31 → 1, the secure one 30 → 2. There is no PRUDP
encryption (TLS carries the traffic). The older raw-UDP format (V1, magic `EA D0`, an HMAC-MD5
on every packet) is in `prudp::v1` as a codec; no Switch game server uses it.

## 3. RMC and the types

An RMC message is a u32 length, then the protocol (bit 0x80 on requests; 0x7F escapes a u16
protocol id) and, for a request, u32 call id, u32 method, parameters; for a response, a bool,
then u32 call id, u32 method | 0x8000 and the result, or u32 error code and u32 call id.

Values are little-endian. String: u16 length counting a NUL, the UTF-8 bytes, the NUL. Buffer:
u32 length; qBuffer: u16 length. List: u32 count. Map: u32 count of key/value pairs. PID: 8
bytes. DateTime: `s | m << 6 | h << 12 | day << 17 | month << 22 | year << 26`. Variant: a type
byte (0 none, 1 i64, 2 f64, 3 bool, 4 String, 5 DateTime, 6 u64) and the value. A structure
writes each class level (base first) as `[u8 version][u32 length][fields]`. An any-data holder:
the class name, u32 (the buffer's length + 4), and the structure in a Buffer.

## 4. Login and the secure connection

- **TicketGranting.ValidateAndRequestTicketWithParam** (0x0A / 6). Parameter (a structure):
  platform type 3, the PID in decimal as the user name, `AuthenticationInfo` (a Data subclass:
  the ID token, NGS version 4, token type 2, server version 0), skip-version-check false, NEX
  version 40605, client version 60 (SMM2's). Answer (structure version 1): our PID, the ticket,
  the secure server's station URL, the server time, the server's name, and the **source key**
  (hex).
- **The ticket**: RC4 with the source key, then an HMAC-MD5 of the ciphertext under the same
  key. Inside: the 32-byte session key, the secure server's PID, and a blob for the secure server
  (ticket version 1: a per-ticket key and the data, keyed with MD5(server key ‖ per-ticket key)).
- **Secure CONNECT**: the payload is Buffer(the blob) + Buffer(the session key's encryption of
  [our PID][the station's CID][a random check value]); the server answers Buffer([check + 1]).
  A station URL whose address is `0.0.0.1` means "the same server".
- **SecureConnection.Register** (0x0B / 1): our station URLs → result, our connection id
  (RVCID) and the public URL the server sees. ReplaceURL is 0x0B / 7.

Station URLs read `prudp:/address=…;port=…;Pl=2;natf=…;natm=…;pmp=0;sid=15;upnp=0` (`type=3` on a
public one). SMCraft registers the game's UDP socket: its LAN address, and with a NAT check its
public address.

## 5. Matchmaking: SMCraft's sessions

| What | Value |
|---|---|
| game mode | `0x5343` ("SC"; no Nintendo title uses it) |
| attributes | `[0x534D4352 "SMCR", protocol 1, kind (0 co-op, 1 versus), course hash low 32 bits, 0, 0]` |
| application data | `SMCRAFT1` + the course hash (u64) |
| participants | 2 to the match size (2–4) |
| search criteria | game mode, the first four attributes, vacant only, no locked or password sessions |

Nextendo matches sessions by game mode (and, where a game server enables it, by the search
criteria), so an SMCraft player is never put in a console's session, and a console never in
SMCraft's. The client checks the session it gets anyway (`MatchmakeSession::is_smcraft`, the
course hash).

Calls used: MatchmakeExtension AutoMatchmakeWithParam_Postpone (0x6D / 40), Create- (38) and
JoinMatchmakeSessionWithParam (39), FindMatchmakeSessionBySingleGatheringId (0x31),
BrowseMatchmakeSessionNoHolderNoResultRange (0x34), Close- / OpenParticipation (1 / 2);
MatchMaking GetSessionURLs (0x15 / 41), GetDetailedParticipants (15), UnregisterGathering (2);
MatchMakingExt EndParticipation (0x32 / 1). The server pushes **notifications** (0x0E / 1:
source PID, type, three u64 parameters and a string; Participate is 3001, its first parameter
the gathering, the second the player) and the client answers each request it gets.

## 6. NAT traversal and the hole punch

- **NAT check** (Nextendo's `nncs` servers, UDP 10025 and 10125): send four big-endian u32
  (test id 101 / 102, 0, 0, our LAN IPv4) from the game socket; the answer is (test id, the
  source port seen, the source IPv4 seen, the server's IPv4). The same external port to both
  ports: one mapping for every destination (natm 1, hole punching works); different ports:
  symmetric (natm 2). Reported with NATTraversal.ReportNATProperties (0x03 / 5).
- **Probe requests**: a joiner asks for the host's station URLs (GetSessionURLs), then
  NATTraversal.RequestProbeInitiationExt (0x03 / 3: the host's URLs, our URL): the server pushes
  InitiateProbe (0x03 / 2, our URL) to the host, so both ends punch at the same moment.
- **SMCraft's punch** on the game's UDP socket: datagrams `"SMCP" 1 kind` + u32 gathering + u64
  sender, then PROBE / REPLY (a nonce), START (the host's player list: PID, IPv4, port each) and
  READY. Each side probes every candidate address of the other; whichever answers is the peer's
  address. When every joiner has answered, the host sends START (player 1 is the host, the
  joiners follow in the order they came), closes the session to newcomers, and the match
  begins. `mmk-netplay` skips `SMCP` datagrams.
- The secure connection stays open (pings) during the match and then leaves the session
  (UnregisterGathering for the host, EndParticipation for a joiner).

Two players who start at the very same moment can each open a session and wait; the server's
matchmaking decides that, as it does for consoles.

## Running it locally

```sh
scripts/nex-local.sh                       # builds core/target/nex-local/nex-local (needs Go)
core/target/nex-local/nex-local -access-key 0f1e2d3c -token-secret localsecret   # prints READY auth=…
mmk-play --no-menus --nextendo-local 127.0.0.1:<port> --nex-access-key 0f1e2d3c --nex-token-secret localsecret
cargo run -p mmk-nex --example peer -- 127.0.0.1:<port> 0f1e2d3c localsecret   # a headless second player
```

The local server is Nextendo's public server core with a small main of ours
(`tools/nex-local/main.go`): authentication and secure servers on 127.0.0.1, game tokens
checked with `-token-secret`, `-tls` for TLS with a throwaway certificate (it prints the key
pin). `cargo test -p mmk-nex` runs the integration tests against it when it is built (they skip
otherwise, as in CI): login, refused tokens and access keys, sessions kept apart from console
ones, TLS with the pin, and two players matched, punched and playing `mmk-netplay` on loopback.

## In the front end

Course World's Network Play button opens SMM2's own Network Play screens (`mmk-front`'s
`netplay`): Co-op or Versus, the search (Quit cancels it and leaves the session), the lobby, the
course, the clear and the result (Exit, or Continue playing for a new search). The host provides
them as a `Network` (`mmk-shell`'s `network`: the search on a thread, the match's UDP link that
also notices a peer leaving). Every player draws the same course from the Story Mode courses of
their import by the gathering ID; the pool's fingerprint is the session's course attribute.

```sh
mmk-play --nextendo-local 127.0.0.1:<port> --nex-access-key 0f1e2d3c --nex-token-secret localsecret --maker Ada
# headless, two of these at once: press Co-op, play the scripted run, capture the end
MMK_FRONT_REALTIME=1 mmk-play --nextendo-local 127.0.0.1:<port> --nex-access-key 0f1e2d3c \
  --nex-token-secret localsecret --front-screen netplay --front-script 10:confirm --scripted \
  --ticks 1400 --screenshot out.png
```

## Not done yet

- Live use: the game server address and SNI, the access key (the game's own, from the player's
  import, or one Nextendo gives SMCraft), the client registration's redirect rules, and
  Nextendo's agreement. Nothing connects to `nextendo.network`'s game servers until then.
- In the front end: the difficulty vote and TGRCode courses, the other players' names, versus
  rating, friend rooms, 3–4 players across NATs (the joiners punch to each other with the host's START list; only tested with
  two on loopback), a relay for symmetric NATs.

// nex-local: a NEX test server on this machine for SMCraft's Nextendo client (core/crates/nex).
//
// It is a small main program around the public nextendo-nex library (Nextendo Network's Go
// NEX/PRUDP server core, PolyForm Shield 1.0.0). None of that library is copied here: the
// build script (scripts/nex-local.sh) fetches it, or points at a local clone, and links it
// into a binary that stays in core/target/. It never talks to nextendo.network.
//
// What it runs, both on 127.0.0.1 only, plain WebSocket (no TLS):
//   - the authentication server (TicketGranting): ValidateAndRequestTicketWithParam and
//     LoginEx. A login is accepted when the ID token in its extra data carries, in its
//     `nnex` claim, an "nx2." game token signed with -token-secret, for the PID that logs
//     in, and not expired: the shape Nextendo's account server issues;
//   - the secure server: SecureConnection, NATTraversal, MatchMaking, MatchmakeExtension and
//     MatchMakingExt from the library, as a game server would register them.
//
// On start it prints one line, "READY auth=<ip:port> secure=<ip:port>", then logs requests.
package main

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"flag"
	"fmt"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	nex "github.com/NextendoNetwork/nextendo-nex"
	"github.com/lxzan/gws"
)

const securePID = 2 // the secure server's account ("Quazal Rendez-Vous")

func main() {
	accessKey := flag.String("access-key", "", "the PRUDP access key clients sign their CONNECT with")
	nexVersion := flag.Int("nex-version", 40605, "the NEX version the server encodes for")
	secret := flag.String("token-secret", "", "the key nx2 game tokens are signed with")
	securePassword := flag.String("secure-password", "", "the secure server account's password (random when empty)")
	flag.Parse()
	if *accessKey == "" || *secret == "" {
		fmt.Fprintln(os.Stderr, "nex-local: -access-key and -token-secret are required")
		os.Exit(2)
	}
	if *securePassword == "" {
		*securePassword = randomHex(16)
	}

	authLn := listen()
	secureLn := listen()
	securePort := secureLn.Addr().(*net.TCPAddr).Port

	settings := nex.NewSwitchSettings(*accessKey, *nexVersion)

	// the secure server
	secure := nex.NewEndpoint(settings)
	secure.SetSecureAccount(*securePassword, securePID)
	mm := nex.NewMatchmaking()
	mm.Endpoint = secure
	secure.Register(nex.ProtocolSecureConnection, nex.SecureConnectionHandler())
	secure.Register(nex.ProtocolNATTraversal, nex.NATTraversalHandler())
	secure.Register(nex.ProtocolMatchMaking, mm.MatchMakingHandler())
	secure.Register(nex.ProtocolMatchmakeExtension, mm.ExtensionHandler())
	secure.Register(nex.ProtocolMatchMakingExt, mm.MatchMakingExtHandler())
	secure.OnDisconnect = func(c *nex.Connection) {
		if c.PID != 0 {
			mm.RemovePlayer(c.PID)
		}
	}
	secure.OnRMC = func(c *nex.Connection, req *nex.RMCMessage) {
		fmt.Printf("[secure] conn=%d proto=0x%x method=%d body=%d bytes\n", c.ID, req.Protocol, req.Method, len(req.Body))
	}

	// the authentication server
	station := nex.ParseStationURL(fmt.Sprintf(
		"prudps:/address=127.0.0.1;port=%d;CID=1;PID=%d;sid=1;stream=10;type=2", securePort, securePID))
	auth := nex.NewEndpoint(settings)
	cfg := &nex.AuthConfig{
		Settings:         settings,
		SecurePID:        securePID,
		SecurePassword:   *securePassword,
		SecureStationURL: station,
		ServerName:       "nex-local",
		ResolveUserEx: func(username string, extraData []byte) (uint64, []byte, uint32) {
			pid, why := checkLogin(username, extraData, []byte(*secret))
			if why != "" {
				fmt.Printf("[auth] login refused: %s\n", why)
				return 0, nil, nex.ResultAuthTokenParseError
			}
			fmt.Printf("[auth] login accepted (pid of %d digits)\n", len(username))
			return pid, randomBytes(16), 0
		},
	}
	auth.Register(nex.ProtocolTicketGranting, cfg.Handler())

	fmt.Printf("READY auth=%s secure=%s\n", authLn.Addr(), secureLn.Addr())
	go serve(secureLn, nex.NewServer(secure))
	serve(authLn, nex.NewServer(auth))
}

func listen() net.Listener {
	ln, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		fmt.Fprintln(os.Stderr, "nex-local:", err)
		os.Exit(1)
	}
	return ln
}

// serve accepts the WebSocket upgrade on any path and hands the socket to the library's
// transport (one PRUDP connection per socket), as its own Listen methods do on 0.0.0.0.
func serve(ln net.Listener, s *nex.Server) {
	up := gws.NewUpgrader(s, &gws.ServerOption{
		ParallelEnabled: false, // PRUDP frames of one connection must be handled in order
		Recovery:        gws.Recovery,
		ReadBufferSize:  64 * 1024,
		WriteBufferSize: 64 * 1024,
	})
	err := http.Serve(ln, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		socket, err := up.Upgrade(w, r)
		if err != nil {
			fmt.Printf("[ws] upgrade refused: %v\n", err)
			return
		}
		go socket.ReadLoop()
	}))
	fmt.Fprintln(os.Stderr, "nex-local:", err)
	os.Exit(1)
}

// checkLogin returns the PID of a valid login, or why it is refused. The game token is
// "nx2." + b64url("pid.username.expiry") + "." + b64url(HMAC-SHA256(secret, "nex:" + payload)).
func checkLogin(username string, extraData []byte, secret []byte) (uint64, string) {
	pid, err := strconv.ParseUint(username, 10, 64)
	if err != nil || pid == 0 {
		return 0, "the user name is not a PID"
	}
	tok, ok := nex.NexTokenFromLoginExtraData(extraData)
	if !ok {
		return 0, "no nnex claim in the ID token"
	}
	if !strings.HasPrefix(tok, "nx2.") {
		return 0, "not an nx2 token"
	}
	parts := strings.SplitN(tok[4:], ".", 2)
	if len(parts) != 2 {
		return 0, "malformed nx2 token"
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		return 0, "malformed nx2 payload"
	}
	mac := hmac.New(sha256.New, secret)
	mac.Write([]byte("nex:" + string(payload)))
	want := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))
	if !hmac.Equal([]byte(want), []byte(parts[1])) {
		return 0, "bad nx2 signature"
	}
	fields := strings.Split(string(payload), ".")
	if len(fields) < 3 {
		return 0, "malformed nx2 payload"
	}
	tokPID, err1 := strconv.ParseUint(fields[0], 10, 64)
	expiry, err2 := strconv.ParseInt(fields[len(fields)-1], 10, 64)
	if err1 != nil || err2 != nil {
		return 0, "malformed nx2 payload"
	}
	if tokPID != pid {
		return 0, "the token is for another PID"
	}
	if time.Now().Unix() > expiry {
		return 0, "the token has expired"
	}
	return pid, ""
}

func randomBytes(n int) []byte {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		panic(err)
	}
	return b
}

func randomHex(n int) string { return fmt.Sprintf("%x", randomBytes(n)) }

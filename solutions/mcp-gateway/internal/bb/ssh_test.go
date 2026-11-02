package bb

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"encoding/pem"
	"errors"
	"log/slog"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

func TestHostKeyAlgorithmsFollowPinnedTypes(t *testing.T) {
	edSigner := mustSigner(t, mustEd25519Key(t))
	rsaSigner := mustSigner(t, mustRSAKey(t))
	edLine := knownhosts.Line([]string{"example.test:22"}, edSigner.PublicKey())
	rsaLine := knownhosts.Line([]string{"[example.test]:2222"}, rsaSigner.PublicKey())

	tests := []struct {
		name    string
		content string
		address string
		want    []string
	}{
		{
			name:    "ed25519 only",
			content: edLine + "\n",
			address: "example.test:22",
			want:    []string{ssh.KeyAlgoED25519},
		},
		{
			name:    "rsa expands to sha2",
			content: rsaLine + "\n",
			address: "example.test:2222",
			want:    []string{ssh.KeyAlgoRSASHA256, ssh.KeyAlgoRSASHA512, ssh.KeyAlgoRSA},
		},
		{
			name:    "port must match",
			content: rsaLine + "\n",
			address: "example.test:22",
			want:    nil,
		},
		{
			name:    "wildcard host",
			content: knownhosts.Line([]string{"*.test"}, edSigner.PublicKey()) + "\n",
			address: "example.test:22",
			want:    []string{ssh.KeyAlgoED25519},
		},
		{
			name:    "hashed host",
			content: knownhosts.Line([]string{knownhosts.HashHostname("example.test")}, edSigner.PublicKey()) + "\n",
			address: "example.test:22",
			want:    []string{ssh.KeyAlgoED25519},
		},
		{
			name:    "unrelated host",
			content: knownhosts.Line([]string{"other.test"}, edSigner.PublicKey()) + "\n",
			address: "example.test:22",
			want:    nil,
		},
		{
			name:    "comment and blank lines",
			content: "# pin\n\n" + edLine + "\n",
			address: "example.test:22",
			want:    []string{ssh.KeyAlgoED25519},
		},
		{
			name:    "revoked entry is not offered",
			content: "@revoked " + edLine + "\n",
			address: "example.test:22",
			want:    nil,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := hostKeyAlgorithmsFor(tc.content, tc.address)
			if strings.Join(got, ",") != strings.Join(tc.want, ",") {
				t.Fatalf("algorithms = %v, want %v", got, tc.want)
			}
		})
	}
}

// TestSSHRunnerNegotiatesPinnedHostKey starts a server that offers RSA before
// ed25519 while known_hosts pins only ed25519. The client must therefore ask
// for ed25519; with the default algorithm list it would negotiate RSA and
// fail host key verification.
func TestSSHRunnerNegotiatesPinnedHostKey(t *testing.T) {
	bridge := startDualHostKeyBridge(t)
	runner := &SSHRunner{
		Address:        bridge.addr,
		User:           "bb",
		KeyFile:        bridge.keyFile,
		KnownHostsFile: bridge.knownHosts,
		Timeout:        5 * time.Second,
	}
	client := NewDispatcher(runner, 5*time.Second)
	status, err := client.Status(context.Background(), 1)
	if err != nil {
		t.Fatalf("Status over SSH: %v", err)
	}
	if len(status.Threads) != 1 {
		t.Fatalf("status = %+v", status)
	}
}

// TestSSHRunnerLogsHostKeyMismatch pins a different ed25519 key from the one
// the server presents and checks that the failure is classified as hostkey.
func TestSSHRunnerLogsHostKeyMismatch(t *testing.T) {
	bridge := startDualHostKeyBridge(t)
	otherSigner := mustSigner(t, mustEd25519Key(t))
	mismatched := filepath.Join(t.TempDir(), "known_hosts")
	line := knownhosts.Line([]string{net.JoinHostPort("127.0.0.1", portOf(t, bridge.addr))}, otherSigner.PublicKey())
	if err := os.WriteFile(mismatched, []byte(line+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	var logs bytes.Buffer
	runner := &SSHRunner{
		Address:        bridge.addr,
		User:           "bb",
		KeyFile:        bridge.keyFile,
		KnownHostsFile: mismatched,
		Timeout:        5 * time.Second,
		Logger:         slog.New(slog.NewTextHandler(&logs, nil)),
	}
	client := NewDispatcher(runner, 5*time.Second)
	_, err := client.Targets(context.Background())
	if err == nil {
		t.Fatal("expected a host key failure")
	}
	got := logs.String()
	if !strings.Contains(got, "class=hostkey") {
		t.Fatalf("log does not classify the failure as hostkey:\n%s", got)
	}
	if strings.Contains(got, "PRIVATE KEY") || strings.Contains(got, "ssh-ed25519 AAAA") {
		t.Fatalf("log leaked key material:\n%s", got)
	}
}

func TestSSHRunnerLogsDialClass(t *testing.T) {
	// Port 1 on loopback is not served by the test, so dialing fails fast.
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := ln.Addr().String()
	if err := ln.Close(); err != nil {
		t.Fatal(err)
	}

	var logs bytes.Buffer
	runner := &SSHRunner{
		Address:        addr,
		User:           "bb",
		KeyFile:        writeClientKey(t),
		KnownHostsFile: emptyKnownHosts(t),
		Timeout:        2 * time.Second,
		Logger:         slog.New(slog.NewTextHandler(&logs, nil)),
	}
	if _, _, err := runner.Run(context.Background(), nil); err == nil {
		t.Fatal("expected a dial failure")
	}
	if !strings.Contains(logs.String(), "class=dial") {
		t.Fatalf("log does not classify the failure as dial:\n%s", logs.String())
	}
}

var errUnknownKey = errors.New("unknown key")

type dualHostKeyBridge struct {
	addr       string
	keyFile    string
	knownHosts string
}

// startDualHostKeyBridge starts an in-process bridge that offers an RSA host
// key first and an ed25519 host key second. known_hosts pins only the ed25519
// key, so a client preferring RSA cannot connect.
func startDualHostKeyBridge(t *testing.T) dualHostKeyBridge {
	t.Helper()
	rsaSigner := mustSigner(t, mustRSAKey(t))
	edSigner := mustSigner(t, mustEd25519Key(t))
	clientPub, clientPriv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	clientPublic := mustPublicKey(t, clientPub)

	config := &ssh.ServerConfig{
		PublicKeyCallback: func(_ ssh.ConnMetadata, key ssh.PublicKey) (*ssh.Permissions, error) {
			if bytes.Equal(key.Marshal(), clientPublic.Marshal()) {
				return nil, nil
			}
			return nil, errUnknownKey
		},
	}
	// Order matters: the RSA key is offered first.
	config.AddHostKey(rsaSigner)
	config.AddHostKey(edSigner)

	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = ln.Close() })
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func(conn net.Conn) {
				defer conn.Close()
				serverConn, channels, requests, err := ssh.NewServerConn(conn, config)
				if err != nil {
					return
				}
				defer serverConn.Close()
				go ssh.DiscardRequests(requests)
				for newChannel := range channels {
					if newChannel.ChannelType() != "session" {
						_ = newChannel.Reject(ssh.UnknownChannelType, "unsupported")
						continue
					}
					ch, reqs, err := newChannel.Accept()
					if err != nil {
						continue
					}
					go serveSession(ch, reqs, func([]byte) (string, int) {
						return `{"ok":true,"result":{"threads":[{"id":"thr_1","title":"Work"}]}}`, 0
					})
				}
			}(conn)
		}
	}()

	addr := ln.Addr().String()
	knownHostsFile := filepath.Join(t.TempDir(), "known_hosts")
	line := knownhosts.Line([]string{addr}, edSigner.PublicKey())
	if err := os.WriteFile(knownHostsFile, []byte(line+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	return dualHostKeyBridge{
		addr:       addr,
		keyFile:    writeClientKeyFrom(t, clientPriv),
		knownHosts: knownHostsFile,
	}
}

func writeClientKey(t *testing.T) string {
	t.Helper()
	_, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return writeClientKeyFrom(t, priv)
}

func writeClientKeyFrom(t *testing.T, priv ed25519.PrivateKey) string {
	t.Helper()
	der, err := x509.MarshalPKCS8PrivateKey(priv)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "id_ed25519")
	block := pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: der})
	if err := os.WriteFile(path, block, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func emptyKnownHosts(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "known_hosts")
	if err := os.WriteFile(path, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func mustSigner(t *testing.T, key any) ssh.Signer {
	t.Helper()
	signer, err := ssh.NewSignerFromKey(key)
	if err != nil {
		t.Fatal(err)
	}
	return signer
}

func mustEd25519Key(t *testing.T) ed25519.PrivateKey {
	t.Helper()
	_, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return priv
}

func mustRSAKey(t *testing.T) *rsa.PrivateKey {
	t.Helper()
	priv, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	return priv
}

func portOf(t *testing.T, addr string) string {
	t.Helper()
	_, port, err := net.SplitHostPort(addr)
	if err != nil {
		t.Fatal(err)
	}
	return port
}

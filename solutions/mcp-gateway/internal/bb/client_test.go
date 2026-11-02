package bb

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/x509"
	"encoding/json"
	"encoding/pem"
	"errors"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

type fakeRunner struct {
	stdout   string
	exitCode int
	err      error
	lastBody []byte
}

func (f *fakeRunner) Run(_ context.Context, payload []byte) ([]byte, int, error) {
	f.lastBody = append([]byte(nil), payload...)
	if f.err != nil {
		return nil, 0, f.err
	}
	return []byte(f.stdout), f.exitCode, nil
}

func TestCallSendsMethodAndDecodesResult(t *testing.T) {
	runner := &fakeRunner{stdout: `{"ok":true,"result":{"threads":[]},"speech":""}`}
	client := NewDispatcher(runner, time.Second)
	if _, err := client.Status(context.Background(), 5); err != nil {
		t.Fatalf("Status: %v", err)
	}
	var sent map[string]any
	if err := json.Unmarshal(runner.lastBody, &sent); err != nil {
		t.Fatalf("request body: %v", err)
	}
	if sent["method"] != MethodStatus {
		t.Fatalf("method = %v", sent["method"])
	}
	params, _ := sent["params"].(map[string]any)
	if params["limit"] != float64(5) {
		t.Fatalf("limit = %v", params["limit"])
	}
}

func TestCallSurfacesRefusalSpeech(t *testing.T) {
	runner := &fakeRunner{
		stdout: `{"ok":false,"code":"thread_limit","speech":"Лимит тредов."}`,
	}
	client := NewDispatcher(runner, time.Second)
	_, err := client.Spawn(context.Background(), SpawnRequest{})
	if err == nil {
		t.Fatal("expected an error")
	}
	got := AsError(err)
	if got.Code != "thread_limit" || got.Speech != "Лимит тредов." {
		t.Fatalf("error = %+v", got)
	}
}

func TestCallTreatsNonZeroExitAsRefusal(t *testing.T) {
	runner := &fakeRunner{
		stdout:   `{"ok":false,"code":"bridge_error","speech":"Сервер агентов недоступен."}`,
		exitCode: 1,
	}
	client := NewDispatcher(runner, time.Second)
	_, err := client.Targets(context.Background())
	if err == nil || AsError(err).Code != "bridge_error" {
		t.Fatalf("err = %v", err)
	}
}

func TestCallHandlesTransportAndGarbage(t *testing.T) {
	client := NewDispatcher(&fakeRunner{err: errors.New("dial")}, time.Second)
	if _, err := client.Targets(context.Background()); AsError(err).Code != "bridge_error" {
		t.Fatalf("transport err = %v", err)
	}
	client = NewDispatcher(&fakeRunner{stdout: "not json"}, time.Second)
	if _, err := client.Targets(context.Background()); AsError(err).Code != "bridge_error" {
		t.Fatalf("garbage err = %v", err)
	}
}

// TestSSHRunnerRunsForcedCommand starts an in-process SSH server that behaves
// like the forced-command bridge and checks the real key + known_hosts path.
func TestSSHRunnerRunsForcedCommand(t *testing.T) {
	addr, keyFile, knownHostsFile := startFakeBridge(t, func(payload []byte) (string, int) {
		var request map[string]any
		if err := json.Unmarshal(payload, &request); err != nil {
			return `{"ok":false,"code":"invalid_request","speech":"Некорректный запрос."}`, 2
		}
		if request["method"] != "status" {
			return `{"ok":false,"code":"unknown_method","speech":"Неизвестная операция."}`, 2
		}
		return `{"ok":true,"result":{"threads":[{"id":"thr_1","title":"Work","profile":"personal","status":"active"}]}}`, 0
	})

	runner := &SSHRunner{
		Address:        addr,
		User:           "bb",
		KeyFile:        keyFile,
		KnownHostsFile: knownHostsFile,
		Timeout:        5 * time.Second,
	}
	client := NewDispatcher(runner, 5*time.Second)
	status, err := client.Status(context.Background(), 10)
	if err != nil {
		t.Fatalf("Status over SSH: %v", err)
	}
	if len(status.Threads) != 1 || status.Threads[0].Title != "Work" {
		t.Fatalf("status = %+v", status)
	}
}

func TestSSHRunnerRejectsUnknownHost(t *testing.T) {
	addr, keyFile, _ := startFakeBridge(t, func([]byte) (string, int) {
		return `{"ok":true,"result":{}}`, 0
	})
	dir := t.TempDir()
	empty := filepath.Join(dir, "known_hosts")
	if err := os.WriteFile(empty, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	runner := &SSHRunner{Address: addr, User: "bb", KeyFile: keyFile, KnownHostsFile: empty, Timeout: 3 * time.Second}
	client := NewDispatcher(runner, 3*time.Second)
	_, err := client.Targets(context.Background())
	if err == nil {
		t.Fatal("expected a host-key failure")
	}
	if strings.Contains(err.Error(), "PRIVATE KEY") {
		t.Fatalf("error leaked key material: %v", err)
	}
}

// startFakeBridge returns the listen address, a client key file and a pinned
// known_hosts file. The server accepts exactly one connection per test and
// answers with handler.
func startFakeBridge(t *testing.T, handler func([]byte) (string, int)) (string, string, string) {
	t.Helper()
	hostPub, hostPriv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	hostSigner, err := ssh.NewSignerFromKey(hostPriv)
	if err != nil {
		t.Fatal(err)
	}
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
			return nil, errors.New("unknown key")
		},
	}
	config.AddHostKey(hostSigner)

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
				for channel := range channels {
					if channel.ChannelType() != "session" {
						_ = channel.Reject(ssh.UnknownChannelType, "unsupported")
						continue
					}
					ch, reqs, err := channel.Accept()
					if err != nil {
						continue
					}
					go serveSession(ch, reqs, handler)
				}
			}(conn)
		}
	}()

	keyPEM, err := x509.MarshalPKCS8PrivateKey(clientPriv)
	if err != nil {
		t.Fatal(err)
	}
	keyFile := filepath.Join(t.TempDir(), "id_ed25519")
	if err := os.WriteFile(keyFile, pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: keyPEM}), 0o600); err != nil {
		t.Fatal(err)
	}
	knownHostsFile := filepath.Join(t.TempDir(), "known_hosts")
	line := knownhosts.Line([]string{ln.Addr().String()}, mustPublicKey(t, hostPub))
	if err := os.WriteFile(knownHostsFile, []byte(line+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	return ln.Addr().String(), keyFile, knownHostsFile
}

func mustPublicKey(t *testing.T, key ed25519.PublicKey) ssh.PublicKey {
	t.Helper()
	public, err := ssh.NewPublicKey(key)
	if err != nil {
		t.Fatal(err)
	}
	return public
}

func serveSession(ch ssh.Channel, reqs <-chan *ssh.Request, handler func([]byte) (string, int)) {
	defer ch.Close()
	for req := range reqs {
		if req.Type != "exec" {
			_ = req.Reply(false, nil)
			continue
		}
		_ = req.Reply(true, nil)
		payload, _ := io.ReadAll(io.LimitReader(ch, 1<<20))
		answer, code := handler(payload)
		_, _ = ch.Write([]byte(answer + "\n"))
		status := struct{ Status uint32 }{Status: uint32(code)}
		_, _ = ch.SendRequest("exit-status", false, ssh.Marshal(&status))
		return
	}
}

package bb

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"os"
	"time"

	"golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

// DefaultTimeout bounds one bridge call, including the SSH handshake.
const DefaultTimeout = 15 * time.Second

// MaxAnswerBytes bounds what the bridge may send back. The bridge itself
// limits responses to 1 MiB.
const MaxAnswerBytes = 1 << 20

// SSHRunner executes the forced command on the bb host. It ignores
// SSH_ORIGINAL_COMMAND on the server side by design: the key has no other
// capability than running the bridge.
type SSHRunner struct {
	// Address is host:port of the bb host.
	Address string
	// User owns the restricted key (normally "bb").
	User string
	// KeyFile is an unencrypted ed25519 private key.
	KeyFile string
	// KnownHostsFile pins the bb host key.
	KnownHostsFile string
	// Timeout bounds one call; DefaultTimeout when zero.
	Timeout time.Duration
	// Logger receives one line per failed call with the error class. Nil
	// means slog.Default().
	Logger *slog.Logger
}

// Run implements Runner.
func (r *SSHRunner) Run(ctx context.Context, payload []byte) ([]byte, int, error) {
	timeout := r.Timeout
	if timeout <= 0 {
		timeout = DefaultTimeout
	}
	callCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	signer, err := loadSigner(r.KeyFile)
	if err != nil {
		return nil, 0, r.fail("auth", err)
	}
	hostKey, err := knownhosts.New(r.KnownHostsFile)
	if err != nil {
		return nil, 0, r.fail("hostkey", fmt.Errorf("bb: load known_hosts: %w", err))
	}
	// Offer only host key types this host actually pins, so the server cannot
	// pick a key we would always reject.
	hostKeyAlgos, err := hostKeyAlgorithms(r.KnownHostsFile, r.Address)
	if err != nil {
		return nil, 0, r.fail("hostkey", err)
	}

	dialer := &net.Dialer{}
	conn, err := dialer.DialContext(callCtx, "tcp", r.Address)
	if err != nil {
		return nil, 0, r.fail("dial", err)
	}
	defer conn.Close()
	if deadline, ok := callCtx.Deadline(); ok {
		_ = conn.SetDeadline(deadline)
	}

	config := &ssh.ClientConfig{
		User:              r.User,
		Auth:              []ssh.AuthMethod{ssh.PublicKeys(signer)},
		HostKeyCallback:   hostKey,
		HostKeyAlgorithms: hostKeyAlgos,
		Timeout:           timeout,
	}
	clientConn, channels, requests, err := ssh.NewClientConn(conn, r.Address, config)
	if err != nil {
		return nil, 0, r.fail(connectErrorClass(err), err)
	}
	defer clientConn.Close()
	client := ssh.NewClient(clientConn, channels, requests)
	defer client.Close()

	session, err := client.NewSession()
	if err != nil {
		return nil, 0, r.fail("session", err)
	}
	defer session.Close()

	stdout := &limitedBuffer{limit: MaxAnswerBytes}
	session.Stdin = bytes.NewReader(payload)
	session.Stdout = stdout
	// stderr is diagnostics only and may contain no secrets; drop it so a
	// bridge warning can never be mistaken for the answer.
	session.Stderr = &limitedBuffer{limit: 4 << 10}

	runErr := session.Run("bb-dispatch")
	if runErr != nil {
		var exitErr *ssh.ExitError
		if errors.As(runErr, &exitErr) {
			// A non-zero exit still carries a JSON answer, so it is only a
			// diagnostic, not a failed call.
			r.logger().Debug("bb ssh bridge exited non-zero",
				"event", "bb_ssh_error",
				"class", "exit_status",
				"address", r.Address,
				"status", exitErr.ExitStatus(),
			)
			return stdout.Bytes(), exitErr.ExitStatus(), nil
		}
		return nil, 0, r.fail("session", runErr)
	}
	return stdout.Bytes(), 0, nil
}

// logger returns the configured logger or the process default.
func (r *SSHRunner) logger() *slog.Logger {
	if r.Logger != nil {
		return r.Logger
	}
	return slog.Default()
}

// fail logs one classified SSH failure and returns err unchanged. Only the
// error class, address and error text are logged: never key material, request
// bodies or answers.
func (r *SSHRunner) fail(class string, err error) error {
	attrs := []any{"event", "bb_ssh_error", "class", class}
	if r.Address != "" {
		attrs = append(attrs, "address", r.Address)
	}
	attrs = append(attrs, "error", err)
	r.logger().Warn("bb ssh call failed", attrs...)
	return err
}

// connectErrorClass names the phase that broke the connection setup.
func connectErrorClass(err error) string {
	var keyErr *knownhosts.KeyError
	var revokedErr *knownhosts.RevokedError
	if errors.As(err, &keyErr) || errors.As(err, &revokedErr) {
		return "hostkey"
	}
	var authErr *ssh.ServerAuthError
	if errors.As(err, &authErr) {
		return "auth"
	}
	return "handshake"
}

func loadSigner(path string) (ssh.Signer, error) {
	key, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("bb: read key: %w", err)
	}
	signer, err := ssh.ParsePrivateKey(key)
	if err != nil {
		return nil, fmt.Errorf("bb: parse key: %w", err)
	}
	return signer, nil
}

// limitedBuffer is a bytes.Buffer that refuses to grow past limit.
type limitedBuffer struct {
	limit int
	buf   bytes.Buffer
}

func (b *limitedBuffer) Write(p []byte) (int, error) {
	remaining := b.limit - b.buf.Len()
	if remaining <= 0 {
		return len(p), nil
	}
	if len(p) > remaining {
		b.buf.Write(p[:remaining])
		return len(p), nil
	}
	b.buf.Write(p)
	return len(p), nil
}

func (b *limitedBuffer) Bytes() []byte { return b.buf.Bytes() }

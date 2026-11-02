package bb

import (
	"bytes"
	"context"
	"errors"
	"fmt"
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
		return nil, 0, err
	}
	hostKey, err := knownhosts.New(r.KnownHostsFile)
	if err != nil {
		return nil, 0, fmt.Errorf("bb: load known_hosts: %w", err)
	}

	dialer := &net.Dialer{}
	conn, err := dialer.DialContext(callCtx, "tcp", r.Address)
	if err != nil {
		return nil, 0, err
	}
	defer conn.Close()
	if deadline, ok := callCtx.Deadline(); ok {
		_ = conn.SetDeadline(deadline)
	}

	config := &ssh.ClientConfig{
		User:            r.User,
		Auth:            []ssh.AuthMethod{ssh.PublicKeys(signer)},
		HostKeyCallback: hostKey,
		Timeout:         timeout,
	}
	clientConn, channels, requests, err := ssh.NewClientConn(conn, r.Address, config)
	if err != nil {
		return nil, 0, err
	}
	defer clientConn.Close()
	client := ssh.NewClient(clientConn, channels, requests)
	defer client.Close()

	session, err := client.NewSession()
	if err != nil {
		return nil, 0, err
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
			return stdout.Bytes(), exitErr.ExitStatus(), nil
		}
		return nil, 0, runErr
	}
	return stdout.Bytes(), 0, nil
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

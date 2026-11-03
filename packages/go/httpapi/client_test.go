package httpapi

import (
	"encoding/pem"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestRedirectsAreNotFollowed(t *testing.T) {
	var followed atomic.Int32
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		followed.Add(1)
		_, _ = w.Write([]byte("secret destination reached"))
	}))
	defer target.Close()

	source := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, target.URL, http.StatusTemporaryRedirect)
	}))
	defer source.Close()

	client, err := NewClient("", 2*time.Second)
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	req, err := http.NewRequest(http.MethodPost, source.URL+"/ricardo.v1.SchedulerService/GetStatus", strings.NewReader("{}"))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer super-secret")
	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("Do: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusTemporaryRedirect {
		t.Errorf("status = %d, want the redirect status to be returned as a failure", resp.StatusCode)
	}
	if got := followed.Load(); got != 0 {
		t.Errorf("redirect target was requested %d times; bearer tokens must not be forwarded", got)
	}
}

func TestTLSVerificationIsPreserved(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	// Without a CA file the self-signed certificate must be rejected.
	strict, err := NewClient("", 2*time.Second)
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if _, err := strict.Get(server.URL); err == nil {
		t.Fatal("self-signed certificate was accepted without a CA file")
	}

	// With the server certificate as a private root the same request works.
	caPath := filepath.Join(t.TempDir(), "ca.pem")
	block := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: server.Certificate().Raw})
	if err := os.WriteFile(caPath, block, 0o600); err != nil {
		t.Fatal(err)
	}
	trusting, err := NewClient(caPath, 2*time.Second)
	if err != nil {
		t.Fatalf("NewClient with CA: %v", err)
	}
	resp, err := trusting.Get(server.URL)
	if err != nil {
		t.Fatalf("request with private CA: %v", err)
	}
	_ = resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Errorf("status = %d, want 200", resp.StatusCode)
	}
}

func TestNewClientRejectsUnusableCAFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ca.pem")
	if err := os.WriteFile(path, []byte("not a certificate"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := NewClient(path, time.Second); err == nil {
		t.Fatal("expected an error for a CA file without certificates")
	}
	if _, err := NewClient(filepath.Join(t.TempDir(), "missing.pem"), time.Second); err == nil {
		t.Fatal("expected an error for a missing CA file")
	}
}

func TestErrorClassification(t *testing.T) {
	cases := []struct {
		name      string
		err       *Error
		transient bool
	}{
		{"network", &Error{Op: "x", Err: errors.New("connection refused")}, true},
		{"timeout", &Error{Op: "x", StatusCode: http.StatusRequestTimeout}, true},
		{"throttled", &Error{Op: "x", StatusCode: http.StatusTooManyRequests}, true},
		{"server", &Error{Op: "x", StatusCode: http.StatusServiceUnavailable}, true},
		{"client", &Error{Op: "x", StatusCode: http.StatusBadRequest}, false},
		{"auth", &Error{Op: "x", StatusCode: http.StatusUnauthorized}, false},
		{"redirect", &Error{Op: "x", StatusCode: http.StatusTemporaryRedirect}, false},
	}
	for _, tc := range cases {
		if got := tc.err.Transient(); got != tc.transient {
			t.Errorf("%s: Transient() = %v, want %v", tc.name, got, tc.transient)
		}
		if got := IsTransient(fmtWrap(tc.err)); got != tc.transient {
			t.Errorf("%s: IsTransient(wrapped) = %v, want %v", tc.name, got, tc.transient)
		}
	}
	if IsTransient(nil) {
		t.Error("nil error must not be transient")
	}
	if IsPermanent(errors.New("plain error")) {
		t.Error("unclassified errors must not be permanent")
	}
}

func TestSanitizeRemovesTokenAndBoundsLength(t *testing.T) {
	got := Sanitize("  bad\ntoken super-secret  here ", "super-secret")
	if strings.Contains(got, "super-secret") {
		t.Errorf("Sanitize left the token in %q", got)
	}
	if strings.Contains(got, "\n") {
		t.Errorf("Sanitize left a newline in %q", got)
	}

	long := Sanitize(strings.Repeat("a", 5000), "")
	if len(long) > maxMessage+4 {
		t.Errorf("Sanitize returned %d bytes, want a bounded message", len(long))
	}
}

func TestStatusCode(t *testing.T) {
	if got := StatusCode(fmtWrap(&Error{StatusCode: 418})); got != 418 {
		t.Errorf("StatusCode = %d, want 418", got)
	}
	if got := StatusCode(errors.New("plain")); got != 0 {
		t.Errorf("StatusCode = %d, want 0", got)
	}
}

func fmtWrap(err error) error {
	return &wrapped{err: err}
}

type wrapped struct{ err error }

func (w *wrapped) Error() string { return "wrapped: " + w.err.Error() }
func (w *wrapped) Unwrap() error { return w.err }

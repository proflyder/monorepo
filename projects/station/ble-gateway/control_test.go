package main

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestControlAuth(t *testing.T) {
	g := newTestGateway(t)
	g.controlToken = "0123456789abcdef0123456789abcdef"
	called := false
	h := g.controlAuth(func(w http.ResponseWriter, r *http.Request) {
		called = true
		w.WriteHeader(http.StatusNoContent)
	})
	for _, tc := range []struct {
		auth string
		code int
		ok   bool
	}{
		{"", http.StatusUnauthorized, false},
		{"Bearer wrong", http.StatusUnauthorized, false},
		{"Bearer " + g.controlToken, http.StatusNoContent, true},
	} {
		req := httptest.NewRequest(http.MethodGet, "/status", nil)
		if tc.auth != "" {
			req.Header.Set("Authorization", tc.auth)
		}
		rr := httptest.NewRecorder()
		h(rr, req)
		if rr.Code != tc.code || called != tc.ok {
			t.Fatalf("auth %q: code=%d called=%v, want %d/%v", tc.auth, rr.Code, called, tc.code, tc.ok)
		}
		called = false
	}
}

func TestControlStatusAndMethods(t *testing.T) {
	installFakeWPA(t, `case "$*" in *" status"*) echo wpa_state=COMPLETED ;; *) echo OK ;; esac`)
	g := newTestGateway(t)
	g.controlToken = "0123456789abcdef0123456789abcdef"

	for _, tc := range []struct {
		method string
		code   int
	}{
		{http.MethodGet, http.StatusOK},
		{http.MethodPost, http.StatusMethodNotAllowed},
	} {
		req := httptest.NewRequest(tc.method, "/status", nil)
		req.Header.Set("Authorization", "Bearer "+g.controlToken)
		rr := httptest.NewRecorder()
		g.controlAuth(g.handleStatus)(rr, req)
		if rr.Code != tc.code {
			t.Fatalf("status %s = %d, want %d", tc.method, rr.Code, tc.code)
		}
		if tc.method == http.MethodGet && !strings.Contains(rr.Body.String(), `"wifi":"COMPLETED"`) {
			t.Fatalf("status body = %s", rr.Body.String())
		}
	}

	for _, tc := range []struct {
		handler http.HandlerFunc
		method  string
		code    int
	}{
		{g.handleGatewayOn, http.MethodGet, http.StatusMethodNotAllowed},
		{g.handleGatewayOn, http.MethodPost, http.StatusOK},
		{g.handleGatewayOff, http.MethodPost, http.StatusOK},
		{g.handleBluetoothOn, http.MethodGet, http.StatusMethodNotAllowed},
		{g.handleBluetoothOff, http.MethodGet, http.StatusMethodNotAllowed},
	} {
		req := httptest.NewRequest(tc.method, "/", nil)
		req.Header.Set("Authorization", "Bearer "+g.controlToken)
		rr := httptest.NewRecorder()
		g.controlAuth(tc.handler)(rr, req)
		if rr.Code != tc.code {
			t.Fatalf("%s = %d, want %d", tc.method, rr.Code, tc.code)
		}
	}
}

func TestLoadControlToken(t *testing.T) {
	g := newTestGateway(t)
	path := filepath.Join(t.TempDir(), "control-token")
	if err := g.loadControlToken(path); err != nil {
		t.Fatal(err)
	}
	if len(g.controlToken) != 64 {
		t.Fatalf("token length = %d", len(g.controlToken))
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0600 {
		t.Fatalf("mode = %o", info.Mode().Perm())
	}
	raw, _ := os.ReadFile(path)
	if strings.TrimSpace(string(raw)) != g.controlToken {
		t.Fatal("token file differs")
	}

	// Existing valid token is reused.
	g2 := newTestGateway(t)
	g2.controlToken = "reuse-me-0123456789"
	if err := os.WriteFile(path, []byte(g2.controlToken+"\n"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := g2.loadControlToken(path); err != nil {
		t.Fatal(err)
	}
	if g2.controlToken != "reuse-me-0123456789" {
		t.Fatalf("token = %q", g2.controlToken)
	}
}

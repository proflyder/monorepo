package main

import (
	"io"
	"log"
	"os"
	"path/filepath"
	"testing"
)

func newTestGateway(t *testing.T) *Gateway {
	t.Helper()
	dir := t.TempDir()
	return &Gateway{
		logger:     log.New(io.Discard, "", 0),
		byName:     make(map[string]*Characteristic),
		status:     "IDLE",
		token:      "123456",
		tokenFile:  filepath.Join(dir, "token"),
		statusFile: filepath.Join(dir, "status"),
		dryRun:     true,
	}
}

func TestTrimToken(t *testing.T) {
	cases := map[string]string{
		"123456":                 "123456",
		" 12-34 56\n":            "123456",
		"abc123def456xyz789012":  "123456789012",
		"":                       "",
		"только текст":           "",
		"0 1 2 3 4 5 6 7 8 9 10": "012345678910",
	}
	for in, want := range cases {
		if got := trimToken(in); got != want {
			t.Errorf("trimToken(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestLoadTokenCreatesAndReadsFile(t *testing.T) {
	g := newTestGateway(t)
	if err := g.loadToken(); err != nil {
		t.Fatalf("loadToken: %v", err)
	}
	if len(g.token) != 6 {
		t.Fatalf("generated token length = %d, want 6", len(g.token))
	}
	raw, err := os.ReadFile(g.tokenFile)
	if err != nil {
		t.Fatalf("read token: %v", err)
	}
	if string(raw) != g.token+"\n" {
		t.Fatalf("token file = %q", raw)
	}
	info, err := os.Stat(g.tokenFile)
	if err != nil {
		t.Fatalf("stat token: %v", err)
	}
	if info.Mode().Perm() != 0600 {
		t.Fatalf("token mode = %o, want 0600", info.Mode().Perm())
	}

	// A pre-existing file must win, including digits mixed with punctuation.
	if err := os.WriteFile(g.tokenFile, []byte(" 987-654 \n"), 0600); err != nil {
		t.Fatal(err)
	}
	g.token = ""
	if err := g.loadToken(); err != nil {
		t.Fatal(err)
	}
	if g.token != "987654" {
		t.Fatalf("loaded token = %q, want 987654", g.token)
	}
}

func TestLoadTokenExistingEmptyFallsBack(t *testing.T) {
	g := newTestGateway(t)
	if err := os.MkdirAll(filepath.Dir(g.tokenFile), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(g.tokenFile, []byte("not digits\n"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := g.loadToken(); err != nil {
		t.Fatal(err)
	}
	if len(g.token) != 6 {
		t.Fatalf("expected generated fallback token, got %q", g.token)
	}
}

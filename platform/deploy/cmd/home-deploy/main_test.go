package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestParsePackArgs(t *testing.T) {
	layout, options, err := parsePackArgs([]string{"/layout", "--ref", "ghcr.io/example/app:tag", "--out", "/tmp/image.tar"})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if layout != "/layout" || options.ref != "ghcr.io/example/app:tag" || options.out != "/tmp/image.tar" {
		t.Fatalf("unexpected parse result: %q %+v", layout, options)
	}

	// Flags before the positional and `--flag=value` also work.
	layout, options, err = parsePackArgs([]string{"--ref=ghcr.io/example/app:tag", "--out=/tmp/x", "/layout"})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if layout != "/layout" || options.ref != "ghcr.io/example/app:tag" || options.out != "/tmp/x" {
		t.Fatalf("unexpected parse result: %q %+v", layout, options)
	}

	for _, args := range [][]string{
		{"--ref", "r", "--out", "o"},                  // no layout
		{"/layout", "--ref", "r"},                     // no --out
		{"/layout", "--out", "o"},                     // no --ref
		{"/a", "/b", "--ref", "r", "--out", "o"},      // two layouts
		{"/layout", "--ref", "r", "--out", "o", "-x"}, // unknown flag
	} {
		if _, _, err := parsePackArgs(args); err == nil {
			t.Fatalf("expected an error for %v", args)
		}
	}
}

func TestRunEnvGetAndBuildID(t *testing.T) {
	path := filepath.Join(t.TempDir(), "scheduler.env")
	if err := os.WriteFile(path, []byte("HA_URL=http://192.0.2.28:8123\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout bytes.Buffer
	if err := run([]string{"env", "get", path, "HA_URL"}, &stdout, &bytes.Buffer{}); err != nil {
		t.Fatalf("env get: %v", err)
	}
	if stdout.String() != "http://192.0.2.28:8123\n" {
		t.Fatalf("env get output = %q", stdout.String())
	}

	stdout.Reset()
	if err := run([]string{"build-id", "--revision", "abc123", "--fingerprint", "image"}, &stdout, &bytes.Buffer{}); err != nil {
		t.Fatalf("build-id: %v", err)
	}
	if !strings.HasPrefix(stdout.String(), "abc123-") || !strings.HasSuffix(stdout.String(), "\n") {
		t.Fatalf("build-id output = %q", stdout.String())
	}
}

func TestRunRejectsUnknownCommand(t *testing.T) {
	if err := run([]string{"nope"}, &bytes.Buffer{}, &bytes.Buffer{}); err == nil {
		t.Fatal("unknown command must fail")
	}
}

package buildid

import (
	"crypto/sha256"
	"encoding/hex"
	"strings"
	"testing"
)

func TestFingerprintMatchesPythonSlice(t *testing.T) {
	// The deploy scripts used Python's sha256(...).hexdigest()[:12]; this is the
	// same value the old heredoc produced for the README fingerprint.
	fingerprint := "ghcr.io/proflyder/scheduler@sha256:" + strings.Repeat("0", 64)
	digest := sha256.Sum256([]byte(fingerprint))
	want := hex.EncodeToString(digest[:])[:12]
	if got := Fingerprint(fingerprint); got != want {
		t.Fatalf("Fingerprint = %q, want %q", got, want)
	}
	if len(Fingerprint(fingerprint)) != ShortLength {
		t.Fatalf("Fingerprint length = %d, want %d", len(Fingerprint(fingerprint)), ShortLength)
	}
}

func TestBuildID(t *testing.T) {
	if got := BuildID("nogit", "abc"); len(got) != len("nogit-")+12 {
		t.Fatalf("BuildID = %q", got)
	}
	if BuildID("abc123", "x") != "abc123-"+Fingerprint("x") {
		t.Fatal("BuildID must join the revision and the fingerprint digest")
	}
}

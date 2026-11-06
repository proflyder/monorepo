// Package golden reads and writes deterministic JSON vectors for the currency
// domain. The committed files are the language-neutral contract: the Go test
// generates them with -update, and the future Rust port replays them unchanged.
//
// A vector is plain JSON produced by json.MarshalIndent, so the comparison is
// byte-exact. Keep non-finite floats out of the vectors (JSON cannot represent
// them); encode them as the strings "NaN", "+Inf" and "-Inf" instead.
//
// The package deliberately owns no flag: a test package that also imports
// contracttest already has a -update flag, and registering a second one would
// panic. Callers pass their own flag value to Run.
package golden

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Run compares got with the JSON file at path, or rewrites the file when
// update is true. The path may be repository-relative or relative to the
// package; a suffix starting at "solutions/" or "testdata/" is also tried so
// the same test works under `go test` (package cwd) and Bazel (runfiles root).
func Run(t *testing.T, update bool, path string, got any) {
	t.Helper()
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	encoder.SetEscapeHTML(false)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(got); err != nil {
		t.Fatalf("encode golden %s: %v", path, err)
	}
	encoded := buffer.Bytes()
	if update {
		write(t, path, encoded)
		return
	}
	want, err := os.ReadFile(Resolve(t, path))
	if err != nil {
		t.Fatalf("read golden %s: %v", path, err)
	}
	if !bytes.Equal(want, encoded) {
		t.Fatalf("golden %s does not match the implementation\n--- want ---\n%s\n--- got ---\n%s", path, want, encoded)
	}
}

// RunText compares a byte-exact text artifact (for example a SQL dump) the
// same way Run handles JSON vectors.
func RunText(t *testing.T, update bool, path, text string) {
	t.Helper()
	if update {
		write(t, path, []byte(text))
		return
	}
	want, err := os.ReadFile(Resolve(t, path))
	if err != nil {
		t.Fatalf("read golden %s: %v", path, err)
	}
	if string(want) != text {
		t.Fatalf("golden %s does not match the implementation\n--- want ---\n%s\n--- got ---\n%s", path, want, text)
	}
}

// Resolve finds an existing testdata file under both go test and Bazel cwds.
func Resolve(t *testing.T, path string) string {
	t.Helper()
	for _, candidate := range candidates(path) {
		if _, err := os.Stat(candidate); err == nil {
			return candidate
		}
	}
	t.Fatalf("golden file not found: %s (tried %s)", path, strings.Join(candidates(path), ", "))
	return ""
}

func write(t *testing.T, path string, data []byte) {
	t.Helper()
	target := writable(path)
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		t.Fatalf("create golden directory: %v", err)
	}
	if err := os.WriteFile(target, data, 0o644); err != nil {
		t.Fatalf("write golden %s: %v", target, err)
	}
}

// writable picks the path to write under -update: the package-relative path
// when its directory exists, otherwise the repository-relative suffix.
func writable(path string) string {
	for _, candidate := range candidates(path) {
		if info, err := os.Stat(filepath.Dir(candidate)); err == nil && info.IsDir() {
			return candidate
		}
	}
	return path
}

func candidates(path string) []string {
	out := []string{path}
	cleaned := filepath.ToSlash(path)
	for _, marker := range []string{"solutions/", "testdata/"} {
		if index := strings.Index(cleaned, marker); index > 0 {
			out = append(out, cleaned[index:])
		}
	}
	return out
}

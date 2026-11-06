// Package golden replays language-neutral vectors of pure functions and
// records their expected output with the shared -update flag.
//
// A vector file is JSON:
//
//	{
//	  "name": "speech",
//	  "description": "why the case matters",
//	  "catalog": "optional YAML for the cases",
//	  "cases": [{"name": "markdown", "input": {...}, "expected": ...}]
//	}
//
// input is passed to the producer as raw JSON; expected is the full result.
// The file is the port contract: the Rust implementation reads the same
// vectors through its own runner.
package golden

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/proflyder/monorepo/packages/go/contracttest"
)

// File is one vectors document.
type File struct {
	Name        string `json:"name"`
	Description string `json:"description"`
	// Catalog optionally carries deployment YAML shared by every case.
	Catalog string `json:"catalog,omitempty"`
	Cases   []Case `json:"cases"`
}

// Case is one input and its expected output.
type Case struct {
	Name     string          `json:"name"`
	Input    json.RawMessage `json:"input"`
	Expected any             `json:"expected"`
}

// Produce turns one case input into its deterministic output.
type Produce func(t *testing.T, file File, input json.RawMessage) (any, error)

// Run replays every case in path. With the shared -update flag it rewrites the
// expected halves and leaves names, descriptions, inputs and the catalog.
func Run(t *testing.T, path string, produce Produce) {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var file File
	if err := json.Unmarshal(raw, &file); err != nil {
		t.Fatalf("decode vectors: %v", err)
	}
	changed := false
	for index := range file.Cases {
		t.Run(file.Cases[index].Name, func(t *testing.T) {
			actual, err := produce(t, file, file.Cases[index].Input)
			if err != nil {
				t.Fatalf("produce: %v", err)
			}
			if *contracttest.Update {
				file.Cases[index].Expected = actual
				changed = true
				return
			}
			if err := compareVector(file.Cases[index].Expected, actual); err != nil {
				t.Fatalf("case %q: %v", file.Cases[index].Name, err)
			}
		})
	}
	if *contracttest.Update && changed {
		var buffer bytes.Buffer
		encoder := json.NewEncoder(&buffer)
		encoder.SetIndent("", "  ")
		encoder.SetEscapeHTML(false)
		if err := encoder.Encode(file); err != nil {
			t.Fatalf("encode vectors: %v", err)
		}
		if err := os.WriteFile(path, buffer.Bytes(), 0o644); err != nil {
			t.Fatalf("write vectors: %v", err)
		}
	}
}

func compareVector(expected, actual any) error {
	encoded, err := json.Marshal(actual)
	if err != nil {
		return fmt.Errorf("encode actual: %w", err)
	}
	decoder := json.NewDecoder(bytes.NewReader(encoded))
	decoder.UseNumber()
	var normalized any
	if err := decoder.Decode(&normalized); err != nil {
		return fmt.Errorf("decode actual: %w", err)
	}
	return contracttest.Compare(expected, normalized)
}

// RelativePath resolves a repository-relative vectors path from either the
// package directory (go test) or the runfiles root (Bazel).
func RelativePath(t *testing.T, path string) string {
	t.Helper()
	candidates := []string{path}
	if index := strings.Index(path, "testdata/"); index >= 0 {
		candidates = append(candidates, path[index:])
	}
	for _, variable := range []string{"TEST_SRCDIR", "RUNFILES_DIR", "GULP_SRCDIR"} {
		root := os.Getenv(variable)
		if root == "" {
			continue
		}
		if workspace := os.Getenv("TEST_WORKSPACE"); workspace != "" {
			candidates = append(candidates, filepath.Join(root, workspace, path))
		}
		candidates = append(candidates, filepath.Join(root, path))
	}
	if root, ok := repositoryRoot(); ok {
		candidates = append(candidates, filepath.Join(root, path))
	}
	for _, candidate := range candidates {
		if _, err := os.Stat(candidate); err == nil {
			return candidate
		}
	}
	t.Fatalf("vectors %s not found (tried %s)", path, strings.Join(candidates, ", "))
	return filepath.Clean(path)
}

func repositoryRoot() (string, bool) {
	dir, err := os.Getwd()
	if err != nil {
		return "", false
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir, true
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", false
		}
		dir = parent
	}
}

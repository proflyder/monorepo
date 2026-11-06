package converge

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"

	yaml "go.yaml.in/yaml/v2"

	"github.com/proflyder/monorepo/platform/deploy/internal/kube"
)

// repoRoot walks up from this test file to the module root (the directory that
// holds go.mod). Under Bazel the test runs from a runfiles tree, so locate()
// prefers TEST_SRCDIR.
func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("cannot resolve the test file path")
	}
	directory := filepath.Dir(file)
	for {
		if _, err := os.Stat(filepath.Join(directory, "go.mod")); err == nil {
			return directory
		}
		parent := filepath.Dir(directory)
		if parent == directory {
			t.Fatal("cannot find the module root above the test file")
		}
		directory = parent
	}
}

func locate(t *testing.T, relative string) string {
	t.Helper()
	if sourceRoot := os.Getenv("TEST_SRCDIR"); sourceRoot != "" {
		for _, workspace := range []string{os.Getenv("TEST_WORKSPACE"), "_main", "yandex_station_lab"} {
			if workspace == "" {
				continue
			}
			candidate := filepath.Join(sourceRoot, workspace, relative)
			if _, err := os.Stat(candidate); err == nil {
				return candidate
			}
		}
		t.Fatalf("runfiles do not contain %s; add it to the go_test data", relative)
	}
	return filepath.Join(repoRoot(t), relative)
}

type traceEntry struct {
	Argv    []string      `json:"argv"`
	Payload []interface{} `json:"payload"`
}

type parityCase struct {
	name             string
	preserveReplicas bool
	preserveEnv      bool
	hasLive          bool
	message          string
}

var parityCases = []parityCase{
	{name: "drift-preserve", preserveReplicas: true, preserveEnv: true, hasLive: true, message: "ha-bridge: converged to manifest"},
	{name: "manifest-wins", hasLive: true, message: "ha-bridge: converged to manifest"},
	{name: "create", message: "ha-bridge: created from manifest"},
	{name: "support-objects", hasLive: true, message: "ha-bridge: converged to manifest"},
}

// TestParityWithPythonTrace runs the Go converge against the committed fake
// kubectl and compares the normalized trace (argv plus parsed payload objects)
// with the fixture recorded from converge.py before it was deleted.
func TestParityWithPythonTrace(t *testing.T) {
	base := locate(t, "platform/deploy/testdata/converge")
	fake := filepath.Join(t.TempDir(), "fake-kubectl")
	script, err := os.ReadFile(filepath.Join(base, "fake-kubectl"))
	if err != nil {
		t.Fatalf("read fake kubectl: %v", err)
	}
	if err := os.WriteFile(fake, script, 0o755); err != nil {
		t.Fatalf("write fake kubectl: %v", err)
	}

	for _, testCase := range parityCases {
		t.Run(testCase.name, func(t *testing.T) {
			directory := filepath.Join(base, testCase.name)
			traceDir := t.TempDir()
			t.Setenv("TRACE_DIR", traceDir)
			if testCase.hasLive {
				t.Setenv("FAKE_LIVE", filepath.Join(directory, "live.json"))
			} else {
				t.Setenv("FAKE_LIVE", "")
			}

			message, err := Converge(Options{
				Kubectl:          fake,
				Namespace:        "home-assistant",
				Name:             "ha-bridge",
				File:             filepath.Join(directory, "manifest.yaml"),
				PreserveReplicas: testCase.preserveReplicas,
				PreserveEnv:      testCase.preserveEnv,
			}, kube.Exec(fake))
			if err != nil {
				t.Fatalf("converge: %v", err)
			}
			if message != testCase.message {
				t.Fatalf("message = %q, want %q", message, testCase.message)
			}

			actual := readTrace(t, traceDir)
			expected := readExpected(t, filepath.Join(directory, "expected.json"))
			if !reflect.DeepEqual(actual, expected) {
				actualJSON, _ := json.MarshalIndent(actual, "", "  ")
				expectedJSON, _ := json.MarshalIndent(expected, "", "  ")
				t.Fatalf("trace differs from the Python fixture\nactual:\n%s\nexpected:\n%s",
					actualJSON, expectedJSON)
			}
		})
	}
}

func readTrace(t *testing.T, directory string) []traceEntry {
	t.Helper()
	entries := []traceEntry{}
	files, err := filepath.Glob(filepath.Join(directory, "*.argv"))
	if err != nil {
		t.Fatalf("list trace: %v", err)
	}
	// The fake names files by a zero-padded index, so lexical order is call order.
	sortStrings(files)
	for _, argvFile := range files {
		raw, err := os.ReadFile(argvFile)
		if err != nil {
			t.Fatalf("read %s: %v", argvFile, err)
		}
		lines := strings.Split(strings.TrimSuffix(string(raw), "\n"), "\n")
		entry := traceEntry{Argv: lines, Payload: []interface{}{}}
		payloadFile := strings.TrimSuffix(argvFile, ".argv") + ".payload"
		if payload, err := os.ReadFile(payloadFile); err == nil {
			entry.Payload = parseDocuments(t, payload)
		}
		entries = append(entries, entry)
	}
	return entries
}

func readExpected(t *testing.T, path string) []traceEntry {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	entries := []traceEntry{}
	if err := json.Unmarshal(raw, &entries); err != nil {
		t.Fatalf("parse %s: %v", path, err)
	}
	// Normalize through JSON so numbers compare as float64 on both sides.
	normalized := []traceEntry{}
	for _, entry := range entries {
		encoded, _ := json.Marshal(entry.Payload)
		var payload []interface{}
		_ = json.Unmarshal(encoded, &payload)
		if payload == nil {
			payload = []interface{}{}
		}
		normalized = append(normalized, traceEntry{Argv: entry.Argv, Payload: payload})
	}
	return normalized
}

func parseDocuments(t *testing.T, raw []byte) []interface{} {
	t.Helper()
	decoder := yaml.NewDecoder(strings.NewReader(string(raw)))
	documents := []interface{}{}
	for {
		var document interface{}
		err := decoder.Decode(&document)
		if err != nil {
			break
		}
		if document == nil {
			continue
		}
		encoded, _ := json.Marshal(normalize(document))
		var normalized interface{}
		_ = json.Unmarshal(encoded, &normalized)
		documents = append(documents, normalized)
	}
	return documents
}

func sortStrings(values []string) {
	for index := 1; index < len(values); index++ {
		for back := index; back > 0 && values[back] < values[back-1]; back-- {
			values[back], values[back-1] = values[back-1], values[back]
		}
	}
}

func TestConvergeRecordsRolloutSummary(t *testing.T) {
	live := `{"metadata":{"generation":4,"resourceVersion":"7"},"spec":{"template":{"metadata":{},"spec":{"containers":[]}}}}`
	manifest := filepath.Join(t.TempDir(), "deployment.yaml")
	document := "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: ha-bridge\n  namespace: home-assistant\nspec:\n  selector:\n    matchLabels:\n      app: bridge\n  template:\n    metadata:\n      labels:\n        app: bridge\n    spec:\n      containers:\n        - name: ha-bridge\n          image: ghcr.io/proflyder/ha-bridge@sha256:" + strings.Repeat("0", 64) + "\n"
	if err := os.WriteFile(manifest, []byte(document), 0o644); err != nil {
		t.Fatal(err)
	}

	for _, testCase := range []struct {
		name       string
		generation string
	}{
		{name: "unchanged", generation: "4"},
		{name: "rolled-out", generation: "5"},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			summary := filepath.Join(t.TempDir(), "rollout-summary")
			calls := [][]string{}
			run := kube.Fake(func(args []string) kube.Result {
				switch {
				case containsArg(args, "replace"):
					return kube.Result{Stdout: "deployment.apps/ha-bridge replaced\n"}
				case containsArg(args, "jsonpath={.metadata.generation}"):
					return kube.Result{Stdout: testCase.generation + "\n"}
				case containsArg(args, "get") && containsArg(args, "json"):
					return kube.Result{Stdout: live}
				default:
					return kube.Result{Code: 1, Stderr: "unexpected kubectl call"}
				}
			}, &calls)
			if _, err := Converge(Options{
				Kubectl:     "fake-kubectl",
				Namespace:   "home-assistant",
				Name:        "ha-bridge",
				File:        manifest,
				SummaryFile: summary,
			}, run); err != nil {
				t.Fatalf("converge: %v", err)
			}
			raw, err := os.ReadFile(summary)
			if err != nil {
				t.Fatalf("read summary: %v", err)
			}
			if got, want := string(raw), "ha-bridge\t"+testCase.name+"\n"; got != want {
				t.Fatalf("summary = %q, want %q", got, want)
			}
		})
	}
}

func containsArg(args []string, want string) bool {
	for _, arg := range args {
		if arg == want {
			return true
		}
	}
	return false
}

// TestDesiredDeploymentReadsTheRenderedName is the Go counterpart of the
// Python test that checked the parsed manifest shape.
func TestDesiredDeploymentReadsTheRenderedName(t *testing.T) {
	path := locate(t, "platform/deploy/testdata/converge/drift-preserve/manifest.yaml")
	objects, err := DesiredObjects(path)
	if err != nil {
		t.Fatalf("desired objects: %v", err)
	}
	deployment, err := DesiredDeployment(objects, path)
	if err != nil {
		t.Fatalf("desired deployment: %v", err)
	}
	metadata, _ := deployment["metadata"].(map[string]interface{})
	if metadata["name"] != "ha-bridge" {
		t.Fatalf("metadata.name = %v", metadata["name"])
	}
}

func TestDesiredDeploymentRejectsAmbiguousFiles(t *testing.T) {
	path := filepath.Join(t.TempDir(), "two.yaml")
	document := "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: one\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: two\n"
	if err := os.WriteFile(path, []byte(document), 0o644); err != nil {
		t.Fatal(err)
	}
	objects, err := DesiredObjects(path)
	if err != nil {
		t.Fatalf("desired objects: %v", err)
	}
	if _, err := DesiredDeployment(objects, path); err == nil {
		t.Fatal("expected an error for two Deployments")
	}
}

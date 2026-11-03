package bb

// Contract test for the gateway -> bb home-dispatcher plugin boundary.
//
// The plugin validates every JSON-RPC method against a strict schema that
// refuses unknown params (solutions/bb-home-dispatcher/lib/schema.js). A live
// bug surfaced this way: validate_spawn was sent the whole SpawnRequest, so
// its optional `title` field made every call fail with
// `invalid_request: Поле params.title не поддерживается.`
//
// solutions/bb-home-dispatcher/contract/params.json is the shared hand-written
// field list. The plugin's JS test pins schema.js to it; this test pins the
// payloads the gateway actually serializes to it, so adding a field to one side
// alone fails a suite.

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

const contractPath = "solutions/bb-home-dispatcher/contract/params.json"

type methodContract struct {
	Required []string `json:"required"`
	Optional []string `json:"optional"`
}

type contractFile struct {
	Version int                       `json:"version"`
	Methods map[string]methodContract `json:"methods"`
}

func loadContract(t *testing.T) contractFile {
	t.Helper()
	path, err := findContract()
	if err != nil {
		t.Fatalf("contract fixture: %v", err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	var parsed contractFile
	if err := json.Unmarshal(raw, &parsed); err != nil {
		t.Fatalf("parse %s: %v", path, err)
	}
	if len(parsed.Methods) == 0 {
		t.Fatalf("%s declares no methods", path)
	}
	return parsed
}

// findContract resolves the fixture both under `go test ./...` (working
// directory is this package) and under `bazel test` (working directory is this
// package inside the runfiles tree, with the file provided through `data`).
func findContract() (string, error) {
	var candidates []string
	if srcDir := os.Getenv("TEST_SRCDIR"); srcDir != "" {
		workspace := os.Getenv("TEST_WORKSPACE")
		candidates = append(candidates, filepath.Join(srcDir, workspace, contractPath))
	}
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for i := 0; i < 8; i++ {
		candidates = append(candidates, filepath.Join(dir, contractPath))
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	for _, candidate := range candidates {
		if _, err := os.Stat(candidate); err == nil {
			return candidate, nil
		}
	}
	return "", fmt.Errorf("%s not found (looked in %d locations)", contractPath, len(candidates))
}

func allowedFields(declared methodContract) map[string]bool {
	allowed := make(map[string]bool, len(declared.Required)+len(declared.Optional))
	for _, field := range declared.Required {
		allowed[field] = true
	}
	for _, field := range declared.Optional {
		allowed[field] = true
	}
	return allowed
}

// TestRequestsUseOnlyContractFields drives every method with a fully populated
// request and checks the serialized params against the fixture.
func TestRequestsUseOnlyContractFields(t *testing.T) {
	contract := loadContract(t)

	fullSpawn := SpawnRequest{
		Profile:        ProfilePersonal,
		ProjectID:      "proj_alpha",
		ProviderID:     "codex",
		Model:          "gpt-5.2-codex",
		ReasoningLevel: "medium",
		Title:          "Fix the flaky test",
		Prompt:         "Find and fix the flaky test.",
		IdempotencyKey: "spawn-1",
	}

	calls := map[string]func(ctx context.Context, d *Dispatcher) error{
		MethodTargets: func(ctx context.Context, d *Dispatcher) error {
			_, err := d.Targets(ctx)
			return err
		},
		MethodStatus: func(ctx context.Context, d *Dispatcher) error {
			_, err := d.Status(ctx, 10)
			return err
		},
		MethodValidateSpawn: func(ctx context.Context, d *Dispatcher) error {
			return d.ValidateSpawn(ctx, fullSpawn)
		},
		MethodSpawn: func(ctx context.Context, d *Dispatcher) error {
			_, err := d.Spawn(ctx, fullSpawn)
			return err
		},
		MethodSend: func(ctx context.Context, d *Dispatcher) error {
			_, err := d.Send(ctx, SendRequest{
				ThreadID:       "thr_1",
				Text:           "hello",
				IdempotencyKey: "send-1",
			})
			return err
		},
		MethodTurnResult: func(ctx context.Context, d *Dispatcher) error {
			_, err := d.TurnResult(ctx, TurnResultRequest{ThreadID: "thr_1", AfterSeq: 3})
			return err
		},
		MethodUpdates: func(ctx context.Context, d *Dispatcher) error {
			afterSeq, waitSeconds := int64(3), 5
			_, err := d.Updates(ctx, UpdatesRequest{AfterSeq: &afterSeq, WaitSeconds: &waitSeconds})
			return err
		},
	}

	if len(calls) != len(contract.Methods) {
		t.Fatalf("test covers %d methods, contract declares %d: %v", len(calls), len(contract.Methods), sortedKeys(contract))
	}
	for method, declared := range contract.Methods {
		call, ok := calls[method]
		if !ok {
			t.Errorf("method %q is declared in the contract but not exercised", method)
			continue
		}
		t.Run(method, func(t *testing.T) {
			runner := &fakeRunner{stdout: `{"ok":true,"result":{}}`}
			if err := call(context.Background(), NewDispatcher(runner, time.Second)); err != nil {
				t.Fatalf("%s: %v", method, err)
			}
			var sent struct {
				Method string          `json:"method"`
				Params json.RawMessage `json:"params"`
			}
			if err := json.Unmarshal(runner.lastBody, &sent); err != nil {
				t.Fatalf("decode request body: %v", err)
			}
			if sent.Method != method {
				t.Fatalf("method = %q, want %q", sent.Method, method)
			}
			params := map[string]any{}
			if len(sent.Params) > 0 {
				if err := json.Unmarshal(sent.Params, &params); err != nil {
					t.Fatalf("decode params: %v", err)
				}
			}
			allowed := allowedFields(declared)
			for field := range params {
				if !allowed[field] {
					t.Errorf("params.%s is not in the plugin schema for %s", field, method)
				}
			}
			for _, field := range declared.Required {
				if _, ok := params[field]; !ok {
					t.Errorf("params.%s is required by the plugin schema for %s", field, method)
				}
			}
		})
	}
}

// TestValidateSpawnSendsOnlyTargetFields is the regression test for the live
// `Поле params.title не поддерживается.` failure.
func TestValidateSpawnSendsOnlyTargetFields(t *testing.T) {
	runner := &fakeRunner{stdout: `{"ok":true,"result":{}}`}
	d := NewDispatcher(runner, time.Second)
	err := d.ValidateSpawn(context.Background(), SpawnAction{
		ProjectID:      "proj_alpha",
		ProviderID:     "codex",
		Model:          "gpt-5.2-codex",
		ReasoningLevel: "medium",
		Title:          "Fix the flaky test",
		Prompt:         "Find and fix the flaky test.",
		IdempotencyKey: "spawn-1",
	}.Request())
	if err != nil {
		t.Fatalf("ValidateSpawn: %v", err)
	}
	var sent struct {
		Params map[string]any `json:"params"`
	}
	if err := json.Unmarshal(runner.lastBody, &sent); err != nil {
		t.Fatalf("decode request body: %v", err)
	}
	for _, forbidden := range []string{"title", "prompt", "idempotencyKey"} {
		if _, ok := sent.Params[forbidden]; ok {
			t.Fatalf("validate_spawn sent %q, which the plugin schema refuses", forbidden)
		}
	}
	want := []string{"model", "profile", "projectId", "providerId", "reasoningLevel"}
	got := make([]string, 0, len(sent.Params))
	for field := range sent.Params {
		got = append(got, field)
	}
	sort.Strings(got)
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Fatalf("validate_spawn params = %v, want %v", got, want)
	}
	if sent.Params["profile"] != ProfilePersonal {
		t.Fatalf("profile = %v", sent.Params["profile"])
	}
}

func sortedKeys(contract contractFile) []string {
	keys := make([]string, 0, len(contract.Methods))
	for key := range contract.Methods {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

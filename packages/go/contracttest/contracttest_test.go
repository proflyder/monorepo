package contracttest

import (
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

func decode(t *testing.T, s string) any {
	t.Helper()
	dec := json.NewDecoder(strings.NewReader(s))
	dec.UseNumber()
	var value any
	if err := dec.Decode(&value); err != nil {
		t.Fatalf("decode %s: %v", s, err)
	}
	return value
}

// fixtureValue mimics encoding/json decoding of a fixture (numbers are
// float64), while responses are decoded with UseNumber.
func fixtureValue(t *testing.T, s string) any {
	t.Helper()
	var value any
	if err := json.Unmarshal([]byte(s), &value); err != nil {
		t.Fatalf("decode fixture %s: %v", s, err)
	}
	return value
}

func TestCompareEqualJSON(t *testing.T) {
	want := fixtureValue(t, `{"count":3,"name":"komp","nested":{"enabled":true},"items":[1,2]}`)
	got := decode(t, `{"count":3,"name":"komp","nested":{"enabled":true},"items":[1,2]}`)
	if err := compare(want, got); err != nil {
		t.Fatalf("compare: %v", err)
	}
}

func TestCompareRejectsDifferentValues(t *testing.T) {
	cases := []struct {
		name string
		want string
		got  string
		note string
	}{
		{"value", `{"name":"a"}`, `{"name":"b"}`, "value mismatch"},
		{"missing key", `{"name":"a","extra":1}`, `{"name":"a"}`, "missing key extra"},
		{"unexpected key", `{"name":"a"}`, `{"name":"a","extra":1}`, "unexpected key extra"},
		{"number", `{"n":1}`, `{"n":2}`, "value mismatch"},
		{"array length", `{"a":[1]}`, `{"a":[1,2]}`, "value mismatch"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := compare(fixtureValue(t, tc.want), decode(t, tc.got))
			if err == nil {
				t.Fatal("compare accepted a mismatch")
			}
			if !strings.Contains(err.Error(), tc.note) {
				t.Fatalf("error = %q, want it to mention %q", err, tc.note)
			}
		})
	}
}

func TestComparePlaceholders(t *testing.T) {
	cases := []struct {
		name string
		want string
		got  string
		ok   bool
	}{
		{"uuid", `{"id":"<uuid>"}`, `{"id":"00000000-0000-4000-8000-000000000001"}`, true},
		{"uuid mismatch", `{"id":"<uuid>"}`, `{"id":"not-a-uuid"}`, false},
		{"base64 nonce", `{"nonce":"<base64-32>"}`, `{"nonce":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="}`, true},
		{"base64 mismatch", `{"nonce":"<base64-32>"}`, `{"nonce":"short"}`, false},
		{"any string", `{"v":"<any>"}`, `{"v":"anything"}`, true},
		{"any number", `{"v":"<any>"}`, `{"v":12}`, true},
		{"any null", `{"v":"<any>"}`, `{"v":null}`, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := compare(fixtureValue(t, tc.want), decode(t, tc.got))
			if tc.ok && err != nil {
				t.Fatalf("compare: %v", err)
			}
			if !tc.ok && err == nil {
				t.Fatal("compare accepted a placeholder mismatch")
			}
		})
	}
}

func TestCompareStringBodiesByteForByte(t *testing.T) {
	if err := compare("data: [DONE]\n\n", "data: [DONE]\n\n"); err != nil {
		t.Fatalf("compare equal strings: %v", err)
	}
	if err := compare("data: [DONE]\n\n", "data: [DONE]\n"); err == nil {
		t.Fatal("compare accepted different byte bodies")
	}
}

func TestCapturePath(t *testing.T) {
	body := decode(t, `{"schedule":{"id":"abc"},"events":[{"id":"e1"},{"id":"e2"}]}`)
	value, ok := lookupPath(body, "schedule.id")
	if !ok || value != "abc" {
		t.Fatalf("schedule.id = %v, %v", value, ok)
	}
	value, ok = lookupPath(body, "events[1].id")
	if !ok || value != "e2" {
		t.Fatalf("events[1].id = %v, %v", value, ok)
	}
	if _, ok := lookupPath(body, "events[2].id"); ok {
		t.Fatal("out-of-range index was found")
	}
	if _, ok := lookupPath(body, "missing.id"); ok {
		t.Fatal("missing path was found")
	}
}

func TestCaptureBodyReportsMissingPath(t *testing.T) {
	step := &fixtureStep{Capture: map[string]string{"eventId": "event.id"}}
	err := captureBody(step, decode(t, `{"event":{}}`), map[string]string{})
	if err == nil || !strings.Contains(err.Error(), "capture eventId") {
		t.Fatalf("error = %v, want a capture error naming eventId", err)
	}
}

func TestReplaceAndResolve(t *testing.T) {
	pattern := regexp.MustCompile(`[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}`)
	value := decode(t, `{"id":"019b796d-d600-77c5-89f1-c06bc947cfc7","nested":["019b796d-d600-77c5-89f1-c06bc947cfc7"]}`)
	replaced := ReplacePattern(value, pattern, "<uuid>")
	if got := replaced.(map[string]any)["id"]; got != "<uuid>" {
		t.Fatalf("id = %v, want <uuid>", got)
	}

	resolved, err := resolve(map[string]any{"id": "{{scheduleId}}"}, map[string]string{"scheduleId": "abc"})
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	if got := resolved.(map[string]any)["id"]; got != "abc" {
		t.Fatalf("resolved id = %v", got)
	}
	if _, err := resolve("{{missing}}", map[string]string{}); err == nil {
		t.Fatal("missing capture was accepted")
	}
}

func TestCompareSemanticAllowsExtraKeysButNotValues(t *testing.T) {
	want := fixtureValue(t, `{"result":{"name":"mcp-gateway","tools":[{"name":"infra_status","annotations":{"readOnlyHint":true}}]}}`)
	got := decode(t, `{"result":{"name":"mcp-gateway","ttlMs":0,"tools":[{"name":"infra_status","annotations":{"readOnlyHint":true,"idempotentHint":false}}]}}`)
	if err := compareWithMode(want, got, true); err != nil {
		t.Fatalf("semantic compare: %v", err)
	}
	if err := compare(want, got); err == nil {
		t.Fatal("exact compare accepted extra keys")
	}
	// Missing expected keys, changed values and array lengths still fail.
	for _, bad := range []string{
		`{"result":{"name":"other","ttlMs":0,"tools":[{"name":"infra_status","annotations":{"readOnlyHint":true}}]}}`,
		`{"result":{"name":"mcp-gateway","ttlMs":0,"tools":[]}}`,
		`{"result":{"name":"mcp-gateway","ttlMs":0}}`,
	} {
		if err := compareWithMode(want, decode(t, bad), true); err == nil {
			t.Fatalf("semantic compare accepted %s", bad)
		}
	}
}

func TestSSEPayloadKeepsTheEnvelopeByteExact(t *testing.T) {
	frame := []byte("event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1}\n\n")
	payload, err := ssePayload(frame)
	if err != nil {
		t.Fatalf("ssePayload: %v", err)
	}
	if string(payload) != `{"jsonrpc":"2.0","id":1}` {
		t.Fatalf("payload = %q", payload)
	}
	for _, broken := range []string{
		"{\"jsonrpc\":\"2.0\"}",
		"event: message\ndata: {}\n",
		"event: ping\ndata: {}\n\n",
	} {
		if _, err := ssePayload([]byte(broken)); err == nil {
			t.Fatalf("ssePayload accepted %q", broken)
		}
	}
}

func TestRunReplaysSemanticSSEFixtures(t *testing.T) {
	dir := t.TempDir()
	fixture := `{
  "name": "semantic",
  "compare": "semantic",
  "steps": [
    {
      "request": {"method": "POST", "path": "/mcp"},
      "response": {
        "status": 200,
        "format": "sse",
        "headers": {"Content-Type": "text/event-stream"},
        "body": {"result": {"name": "mcp-gateway", "tools": [{"name": "infra_status"}]}}
      }
    }
  ]
}`
	if err := os.WriteFile(filepath.Join(dir, "semantic.json"), []byte(fixture), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
	Run(t, dir, func(t *testing.T, scenario string) http.Handler {
		if scenario != "semantic" {
			t.Fatalf("scenario = %q", scenario)
		}
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			_, _ = io.WriteString(w, "event: message\ndata: "+`{"result":{"name":"mcp-gateway","ttlMs":0,"tools":[{"name":"infra_status","inputSchema":{"type":"object"}}]}}`+"\n\n")
		})
	})
}

func TestRunReplaysFixtures(t *testing.T) {
	dir := t.TempDir()
	fixture := `{
  "name": "echo",
  "description": "echo the captured id",
  "steps": [
    {
      "request": {"method": "POST", "path": "/echo", "headers": {"Content-Type": "application/json"}, "body": {"id": "fixed"}},
      "capture": {"id": "id"},
      "response": {"status": 200, "headers": {"Content-Type": "application/json"}, "body": {"id": "{{id}}", "seen": true}}
    },
    {
      "request": {"method": "POST", "path": "/echo", "headers": {"Content-Type": "application/json"}, "body": {"id": "{{id}}"}},
      "response": {"status": 200, "headers": {"Content-Type": "application/json"}, "body": {"id": "fixed", "seen": true}}
    }
  ]
}`
	if err := os.WriteFile(filepath.Join(dir, "echo.json"), []byte(fixture), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
	Run(t, dir, func(t *testing.T, scenario string) http.Handler {
		if scenario != "echo" {
			t.Fatalf("scenario = %q", scenario)
		}
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var body map[string]string
			_ = json.NewDecoder(r.Body).Decode(&body)
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{"id": body["id"], "seen": true})
		})
	})
}

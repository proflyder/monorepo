package preflight

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLegacyWriterFailuresAbsentIsSafe(t *testing.T) {
	if failures := LegacyWriterFailures(map[string]string{}); len(failures) != 0 {
		t.Fatalf("absent writers must be safe: %v", failures)
	}
}

func TestLegacyWriterFailuresOffIsSafe(t *testing.T) {
	states := map[string]string{}
	for _, name := range LegacyWriters {
		states[name] = "off"
	}
	if failures := LegacyWriterFailures(states); len(failures) != 0 {
		t.Fatalf("off writers must be safe: %v", failures)
	}
}

func TestLegacyWriterFailuresMixedAbsentAndOffIsSafe(t *testing.T) {
	states := map[string]string{
		LegacyWriters[0]: "off",
		LegacyWriters[2]: "off",
	}
	if failures := LegacyWriterFailures(states); len(failures) != 0 {
		t.Fatalf("mixed absent/off writers must be safe: %v", failures)
	}
}

func TestLegacyWriterFailuresOnFails(t *testing.T) {
	states := map[string]string{}
	for _, name := range LegacyWriters {
		states[name] = "off"
	}
	states[LegacyWriters[0]] = "on"
	failures := LegacyWriterFailures(states)
	if len(failures) != 1 {
		t.Fatalf("expected one failure, got %v", failures)
	}
	if !strings.Contains(failures[0], LegacyWriters[0]) {
		t.Fatalf("failure does not name the writer: %q", failures[0])
	}
}

func TestLegacyWriterFailuresUnknownAndUnavailableFail(t *testing.T) {
	for _, bad := range []string{"unknown", "unavailable"} {
		failures := LegacyWriterFailures(map[string]string{LegacyWriters[1]: bad})
		if len(failures) != 1 {
			t.Fatalf("%s: expected one failure, got %v", bad, failures)
		}
		if !strings.Contains(failures[0], LegacyWriters[1]) {
			t.Fatalf("%s: failure does not name the writer: %q", bad, failures[0])
		}
	}
}

func TestLegacyWriterFailuresUnexpectedStateFails(t *testing.T) {
	failures := LegacyWriterFailures(map[string]string{LegacyWriters[3]: "idle"})
	if len(failures) != 1 {
		t.Fatalf("expected one failure, got %v", failures)
	}
}

// fakeHome is a Home Assistant + scheduler API for the run tests.
type fakeHome struct {
	states    []map[string]interface{}
	statesErr int
	status    map[string]interface{}
	statusErr int
	server    *httptest.Server
}

func newFakeHome(t *testing.T) *fakeHome {
	t.Helper()
	home := &fakeHome{states: []map[string]interface{}{}, status: map[string]interface{}{"ready": true}}
	home.server = httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer token" {
			http.Error(writer, "unauthorized", http.StatusUnauthorized)
			return
		}
		switch request.URL.Path {
		case "/api/states":
			if home.statesErr != 0 {
				http.Error(writer, "boom", home.statesErr)
				return
			}
			_ = json.NewEncoder(writer).Encode(home.states)
		case "/ricardo.v1.SchedulerService/GetStatus":
			if home.statusErr != 0 {
				http.Error(writer, "boom", home.statusErr)
				return
			}
			_ = json.NewEncoder(writer).Encode(home.status)
		default:
			http.NotFound(writer, request)
		}
	}))
	t.Cleanup(home.server.Close)
	return home
}

func (home *fakeHome) run(t *testing.T, expectedEndpoint string) (int, string) {
	t.Helper()
	directory := t.TempDir()
	haToken := filepath.Join(directory, "ha.token")
	schedulerToken := filepath.Join(directory, "scheduler.token")
	if err := os.WriteFile(haToken, []byte(" token \n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(schedulerToken, []byte("token\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout bytes.Buffer
	status := Run(Options{
		HAURL:              home.server.URL,
		HATokenFile:        haToken,
		SchedulerURL:       home.server.URL,
		SchedulerTokenFile: schedulerToken,
		ExpectedEndpoint:   expectedEndpoint,
		Stdout:             &stdout,
	})
	return status, stdout.String()
}

func state(entity, value string) map[string]interface{} {
	return map[string]interface{}{"entity_id": entity, "state": value}
}

func TestRunSucceedsWithAbsentWriters(t *testing.T) {
	home := newFakeHome(t)
	status, output := home.run(t, "")
	if status != 0 {
		t.Fatalf("status = %d, output: %s", status, output)
	}
	if !strings.Contains(output, "preflight ok:") {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestRunSucceedsWithOffWriters(t *testing.T) {
	home := newFakeHome(t)
	for _, name := range LegacyWriters {
		home.states = append(home.states, state(name, "off"))
	}
	home.states = append(home.states,
		state("input_boolean.alarm_ringing", "off"),
		state("input_text.scheduler_endpoint", "http://192.0.2.27:30098"),
	)
	status, output := home.run(t, "http://192.0.2.27:30098")
	if status != 0 {
		t.Fatalf("status = %d, output: %s", status, output)
	}
}

func TestRunFailsOnOnWriter(t *testing.T) {
	home := newFakeHome(t)
	home.states = append(home.states, state(LegacyWriters[0], "on"))
	status, output := home.run(t, "")
	if status != 1 {
		t.Fatalf("status = %d, want 1", status)
	}
	if !strings.Contains(output, "legacy writer "+LegacyWriters[0]+" is 'on'") {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestRunFailsOnUnknownWriter(t *testing.T) {
	for _, bad := range []string{"unknown", "unavailable"} {
		home := newFakeHome(t)
		home.states = append(home.states, state(LegacyWriters[1], bad))
		status, output := home.run(t, "")
		if status != 1 {
			t.Fatalf("%s: status = %d, want 1", bad, status)
		}
		if !strings.Contains(output, "is '"+bad+"'") {
			t.Fatalf("%s: unexpected output: %q", bad, output)
		}
	}
}

func TestRunFailsWhenHAStatesUnavailable(t *testing.T) {
	home := newFakeHome(t)
	home.statesErr = http.StatusInternalServerError
	status, output := home.run(t, "")
	if status != 1 {
		t.Fatalf("status = %d, want 1", status)
	}
	if !strings.Contains(output, "FAIL: cannot read HA states (HTTP 500)") {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestRunFailsWhenAlarmIsRinging(t *testing.T) {
	home := newFakeHome(t)
	home.states = append(home.states, state("input_boolean.alarm_ringing", "on"))
	status, output := home.run(t, "")
	if status != 1 {
		t.Fatalf("status = %d, want 1", status)
	}
	if !strings.Contains(output, "an alarm is ringing in HA") {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestRunFailsOnEndpointMismatch(t *testing.T) {
	home := newFakeHome(t)
	home.states = append(home.states, state("input_text.scheduler_endpoint", "http://192.0.2.27:30098"))
	status, output := home.run(t, "http://192.0.2.28:30098")
	if status != 1 {
		t.Fatalf("status = %d, want 1", status)
	}
	if !strings.Contains(output, "input_text.scheduler_endpoint is") {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestRunFailsWhenSchedulerUnavailable(t *testing.T) {
	home := newFakeHome(t)
	home.statusErr = http.StatusServiceUnavailable
	status, output := home.run(t, "")
	if status != 1 {
		t.Fatalf("status = %d, want 1", status)
	}
	if !strings.Contains(output, "scheduler GetStatus failed (HTTP 503)") {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestRunFailsWhenSchedulerNotReady(t *testing.T) {
	home := newFakeHome(t)
	home.status = map[string]interface{}{"ready": false}
	status, output := home.run(t, "")
	if status != 1 {
		t.Fatalf("status = %d, want 1", status)
	}
	if !strings.Contains(output, "scheduler reports ready=false") {
		t.Fatalf("unexpected output: %q", output)
	}
}

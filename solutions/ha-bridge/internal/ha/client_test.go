package ha

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
)

const testToken = "ha-secret-token"

func boolPtr(t *testing.T, variables map[string]any) bool {
	t.Helper()
	value, ok := variables["chime"].(bool)
	if !ok {
		t.Fatalf("chime = %#v, want bool", variables["chime"])
	}
	return value
}

func TestCallScriptRequestShape(t *testing.T) {
	var paths []string
	var bodies []map[string]any
	var headers []http.Header
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		headers = append(headers, r.Header.Clone())
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Errorf("decode body: %v", err)
		}
		bodies = append(bodies, body)
		fmt.Fprint(w, `[]`)
	}))
	defer server.Close()

	client := New(server.URL, testToken, server.Client())
	err := client.CallScript(context.Background(), "schedule_announce", map[string]any{
		"message": "Пицца готова",
		"chime":   true,
	})
	if err != nil {
		t.Fatalf("CallScript: %v", err)
	}

	if len(paths) != 1 || paths[0] != "/api/services/script/turn_on" {
		t.Errorf("paths = %v", paths)
	}
	if got := headers[0].Get("Authorization"); got != "Bearer "+testToken {
		t.Errorf("Authorization = %q", got)
	}
	if got := headers[0].Get("Content-Type"); got != "application/json" {
		t.Errorf("Content-Type = %q", got)
	}
	if bodies[0]["entity_id"] != "script.schedule_announce" {
		t.Errorf("entity_id = %v", bodies[0]["entity_id"])
	}
	variables, ok := bodies[0]["variables"].(map[string]any)
	if !ok {
		t.Fatalf("variables = %#v", bodies[0]["variables"])
	}
	if variables["message"] != "Пицца готова" || !boolPtr(t, variables) {
		t.Errorf("variables = %#v", variables)
	}
}

func TestCallScriptAcceptsDottedEntity(t *testing.T) {
	var entity string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		entity, _ = body["entity_id"].(string)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := New(server.URL, testToken, server.Client())
	if err := client.CallScript(context.Background(), "script.alarm_stop", nil); err != nil {
		t.Fatalf("CallScript: %v", err)
	}
	if entity != "script.alarm_stop" {
		t.Errorf("entity = %q, want no double prefix", entity)
	}
}

func TestCallScriptErrorClassification(t *testing.T) {
	cases := []struct {
		name      string
		status    int
		transient bool
	}{
		{"server error", http.StatusInternalServerError, true},
		{"unavailable", http.StatusServiceUnavailable, true},
		{"throttled", http.StatusTooManyRequests, true},
		{"bad request", http.StatusBadRequest, false},
		{"not found", http.StatusNotFound, false},
		{"unauthorized", http.StatusUnauthorized, false},
	}
	for _, tc := range cases {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(tc.status)
			fmt.Fprint(w, `{"message":"entity not found"}`)
		}))
		client := New(server.URL, testToken, server.Client())
		err := client.CallScript(context.Background(), "alarm_start", nil)
		server.Close()
		if err == nil {
			t.Fatalf("%s: expected an error", tc.name)
		}
		if got := httpapi.IsTransient(err); got != tc.transient {
			t.Errorf("%s: transient = %v, want %v", tc.name, got, tc.transient)
		}
		if strings.Contains(err.Error(), testToken) {
			t.Errorf("%s: error leaked the token", tc.name)
		}
	}
}

func TestPingUsesReadOnlyEndpoint(t *testing.T) {
	var method, path, auth string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		method, path, auth = r.Method, r.URL.Path, r.Header.Get("Authorization")
		fmt.Fprint(w, `{"message":"API running."}`)
	}))
	defer server.Close()

	client := New(server.URL, testToken, server.Client())
	if err := client.Ping(context.Background()); err != nil {
		t.Fatalf("Ping: %v", err)
	}
	if method != http.MethodGet || path != "/api/" {
		t.Errorf("%s %s, want GET /api/", method, path)
	}
	if auth != "Bearer "+testToken {
		t.Errorf("Authorization = %q", auth)
	}
}

func TestPingRejectsUnauthorized(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer server.Close()

	client := New(server.URL, testToken, server.Client())
	err := client.Ping(context.Background())
	if err == nil {
		t.Fatal("expected an error")
	}
	if httpapi.IsTransient(err) {
		t.Error("401 is not transient")
	}
}

func TestRedirectIsNotFollowed(t *testing.T) {
	var followed atomic.Int32
	destination := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		followed.Add(1)
		w.WriteHeader(http.StatusOK)
	}))
	defer destination.Close()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, destination.URL, http.StatusFound)
	}))
	defer server.Close()

	client := New(server.URL, testToken, server.Client())
	err := client.CallScript(context.Background(), "alarm_start", nil)
	if err == nil {
		t.Fatal("a redirect must not count as an accepted call")
	}
	if followed.Load() != 0 {
		t.Fatal("redirect target was requested; the bearer token must not be forwarded")
	}
}

func TestNetworkErrorOmitsURLAndToken(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	url := server.URL
	server.Close()

	client := New(url, testToken, &http.Client{Timeout: time.Second})
	err := client.CallScript(context.Background(), "alarm_start", nil)
	if err == nil {
		t.Fatal("expected an error")
	}
	if strings.Contains(err.Error(), url) || strings.Contains(err.Error(), testToken) {
		t.Errorf("error leaked URL or token: %q", err.Error())
	}
	if !httpapi.IsTransient(err) {
		t.Error("a network failure must be transient")
	}
}

func TestEntityID(t *testing.T) {
	cases := map[string]string{
		"alarm_start":        "script.alarm_start",
		"script.alarm_start": "script.alarm_start",
		"  alarm_stop  ":     "script.alarm_stop",
	}
	for input, want := range cases {
		if got := EntityID(input); got != want {
			t.Errorf("EntityID(%q) = %q, want %q", input, got, want)
		}
	}
}

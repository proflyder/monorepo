package checks

import (
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
)

func TestTCPCheck(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	defer server.Close()
	host, port, err := net.SplitHostPort(strings.TrimPrefix(server.URL, "http://"))
	if err != nil {
		t.Fatal(err)
	}
	n, _ := strconv.Atoi(port)
	runner := New("", "", nil)

	result := runner.Run(context.Background(), "svc", catalog.Check{Type: catalog.CheckTCP, Host: host, Port: n, Label: "ts"})
	if !result.OK || result.Error != "" {
		t.Fatalf("result = %+v", result)
	}

	// A port that was just closed refuses the connection.
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	closedPort := ln.Addr().(*net.TCPAddr).Port
	ln.Close()
	result = runner.Run(context.Background(), "svc", catalog.Check{Type: catalog.CheckTCP, Host: "127.0.0.1", Port: closedPort, Label: "closed"})
	if result.OK || result.Error == "" {
		t.Fatalf("closed port result = %+v", result)
	}
}

func TestHTTPCheckStatusAndAuth(t *testing.T) {
	var gotAuth string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		if r.URL.Path == "/missing" {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()
	runner := New(server.URL, "ha-token", nil)

	result := runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHTTP, URL: server.URL + "/", ExpectStatus: http.StatusNoContent, Auth: catalog.AuthHA, Label: "api",
	})
	if !result.OK || result.Value != "HTTP 204" {
		t.Fatalf("result = %+v", result)
	}
	if gotAuth != "Bearer ha-token" {
		t.Fatalf("auth header = %q", gotAuth)
	}

	result = runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHTTP, URL: server.URL + "/missing", ExpectStatus: http.StatusOK, Label: "missing",
	})
	if result.OK || !strings.Contains(result.Error, "404") {
		t.Fatalf("mismatch result = %+v", result)
	}
}

func TestHAStateCheck(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/states/sensor.demo" {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]string{"entity_id": "sensor.demo", "state": "off"})
	}))
	defer server.Close()
	runner := New(server.URL, "token", nil)

	result := runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAState, EntityID: "sensor.demo", Label: "demo",
	})
	if !result.OK || result.Value != "off" {
		t.Fatalf("result = %+v", result)
	}

	result = runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAState, EntityID: "sensor.demo", BadStates: []string{"off"}, Label: "demo",
	})
	if result.OK || !strings.Contains(result.Error, "off") {
		t.Fatalf("bad-state result = %+v", result)
	}

	result = runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAState, EntityID: "sensor.missing", Label: "missing",
	})
	if result.OK || !strings.Contains(result.Error, "404") {
		t.Fatalf("missing result = %+v", result)
	}
}

func TestHAChecksAreSkippedWhenNotConfigured(t *testing.T) {
	runner := New("", "", nil)
	for _, check := range []catalog.Check{
		{Type: catalog.CheckHAState, EntityID: "sensor.x", Label: "state"},
		{Type: catalog.CheckHATemplate, Template: "{{ 1 }}", Label: "tpl"},
		{Type: catalog.CheckHAUnavailable, Label: "unavailable"},
		{Type: catalog.CheckHTTP, URL: "http://example.invalid/", Auth: catalog.AuthHA, Label: "http"},
	} {
		result := runner.Run(context.Background(), "ha", check)
		if !result.OK || !result.Skipped {
			t.Fatalf("%s result = %+v", check.Type, result)
		}
		if !strings.Contains(result.Value, "не настроен") {
			t.Fatalf("%s value = %q", check.Type, result.Value)
		}
	}
}

func TestHATemplateCheck(t *testing.T) {
	current := "1.2.3"
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/api/template" {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		if r.Header.Get("Authorization") != "Bearer token" {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		_, _ = w.Write([]byte(current))
	}))
	defer server.Close()
	runner := New(server.URL, "token", nil)

	result := runner.Run(context.Background(), "svc", catalog.Check{
		Type: catalog.CheckHATemplate, Template: "{{ x }}", Label: "version",
	})
	if !result.OK || result.Value != "1.2.3" {
		t.Fatalf("result = %+v", result)
	}

	current = ""
	result = runner.Run(context.Background(), "svc", catalog.Check{
		Type: catalog.CheckHATemplate, Template: "{{ x }}", Label: "version",
	})
	if !result.OK {
		t.Fatalf("optional empty result = %+v", result)
	}
	result = runner.Run(context.Background(), "svc", catalog.Check{
		Type: catalog.CheckHATemplate, Template: "{{ x }}", Required: true, Label: "version",
	})
	if result.OK || !strings.Contains(result.Error, "пустое") {
		t.Fatalf("required empty result = %+v", result)
	}
}

func TestHAUnavailableSummary(t *testing.T) {
	states := []map[string]string{
		{"entity_id": "media_player.one", "state": "unavailable"},
		{"entity_id": "media_player.two", "state": "unavailable"},
		{"entity_id": "assist_satellite.three", "state": "unavailable"},
		{"entity_id": "light.one", "state": "on"},
		{"entity_id": "sensor.one", "state": "unavailable"},
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/states" {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		_ = json.NewEncoder(w).Encode(states)
	}))
	defer server.Close()
	runner := New(server.URL, "token", nil)

	result := runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAUnavailable, Domains: []string{"media_player", "assist_satellite"}, Label: "unavailable",
	})
	if result.OK || result.Value != "3" || len(result.Details) != 3 {
		t.Fatalf("result = %+v", result)
	}

	result = runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAUnavailable, Label: "all",
	})
	if result.Value != "4" {
		t.Fatalf("all result = %+v", result)
	}
}

func TestHAUnavailableDetailPrefersFriendlyName(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, `[
  {"entity_id":"media_player.kitchen","state":"unavailable","attributes":{"friendly_name":"Кухня"}},
  {"entity_id":"media_player.hall","state":"unavailable"}
]`)
	}))
	defer server.Close()
	result := New(server.URL, "token", nil).Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAUnavailable, Label: "недоступные", Domains: []string{"media_player"},
	})
	if result.Value != "2" {
		t.Fatalf("result = %+v", result)
	}
	want := []string{"Кухня (media_player.kitchen)", "media_player.hall"}
	if len(result.Details) != len(want) {
		t.Fatalf("details = %v", result.Details)
	}
	for i := range want {
		if result.Details[i] != want[i] {
			t.Fatalf("details[%d] = %q, want %q", i, result.Details[i], want[i])
		}
	}
}

func TestHAUnavailableDetailIsCapped(t *testing.T) {
	var states []map[string]string
	for i := 0; i < 25; i++ {
		states = append(states, map[string]string{"entity_id": "sensor.s" + strconv.Itoa(i), "state": "unavailable"})
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(states)
	}))
	defer server.Close()
	result := New(server.URL, "token", nil).Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHAUnavailable, Label: "all",
	})
	if result.Value != "25" || len(result.Details) != 10 {
		t.Fatalf("result = %+v", result)
	}
}

func TestCheckTimeoutIsBounded(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(500 * time.Millisecond)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()
	runner := New(server.URL, "token", nil).WithTimeout(80 * time.Millisecond)
	start := time.Now()
	result := runner.Run(context.Background(), "ha", catalog.Check{
		Type: catalog.CheckHTTP, URL: server.URL + "/", ExpectStatus: http.StatusOK, Label: "slow",
	})
	if result.OK {
		t.Fatalf("timeout result = %+v", result)
	}
	if elapsed := time.Since(start); elapsed > 400*time.Millisecond {
		t.Fatalf("check took %v", elapsed)
	}
}

func TestRunAllKeepsCatalogOrder(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()
	services := []catalog.Service{
		{ID: "first", Checks: []catalog.Check{
			{Type: catalog.CheckHTTP, URL: server.URL + "/a", Label: "a"},
			{Type: catalog.CheckHTTP, URL: server.URL + "/b", Label: "b"},
		}},
		{ID: "second", Checks: []catalog.Check{
			{Type: catalog.CheckTCP, Host: "127.0.0.1", Port: 1, Label: "closed"},
		}},
	}
	results := New(server.URL, "", nil).RunAll(context.Background(), services)
	if len(results) != 3 {
		t.Fatalf("results = %d", len(results))
	}
	if results[0].Label != "a" || results[1].Label != "b" || results[2].Label != "closed" {
		t.Fatalf("order = %v", []string{results[0].Label, results[1].Label, results[2].Label})
	}
	if !results[0].OK || !results[1].OK || results[2].OK {
		t.Fatalf("results = %+v", results)
	}
}

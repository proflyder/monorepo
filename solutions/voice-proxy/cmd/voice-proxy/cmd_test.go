package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/config"
)

// TestStatusHasNoToken pins the distroless diagnostic contract: -status carries
// switches and entity ids, never a secret value.
func TestStatusHasNoToken(t *testing.T) {
	const (
		haToken        = "ha-secret-value"
		gatewayToken   = "gateway-secret-value"
		schedulerToken = "scheduler-secret-value"
	)
	cfg := config.Config{
		Bind:            "127.0.0.1",
		Port:            8643,
		Upstream:        "http://127.0.0.1:8642",
		ReasoningEffort: "low",
		HAURL:           "http://ha.example",
		HAToken:         haToken,
		GatewayURL:      "http://gateway.example",
		GatewayToken:    gatewayToken,
		SchedulerURL:    "http://scheduler.example",
		SchedulerToken:  schedulerToken,
		ProfileEntity:   "input_select.voice_profile",
	}
	var buf strings.Builder
	if err := writeStatus(&buf, cfg); err != nil {
		t.Fatalf("writeStatus: %v", err)
	}
	out := buf.String()
	for _, secret := range []string{haToken, gatewayToken, schedulerToken} {
		if strings.Contains(out, secret) {
			t.Fatalf("status output leaked a token: %q", out)
		}
	}
	var decoded map[string]any
	if err := json.Unmarshal([]byte(out), &decoded); err != nil {
		t.Fatalf("status output is not JSON: %v\n%s", err, out)
	}
	if decoded["routing_enabled"] != true {
		t.Errorf("routing_enabled = %v, want true", decoded["routing_enabled"])
	}
	if decoded["scheduler_configured"] != true {
		t.Errorf("scheduler_configured = %v, want true", decoded["scheduler_configured"])
	}
}

func TestStatusFailsOnInvalidConfiguration(t *testing.T) {
	t.Setenv("HERMES_PROXY_PORT", "not-a-port")
	if code := runStatus(); code != 1 {
		t.Fatalf("runStatus = %d, want 1", code)
	}
}

func TestHealthcheckProbesTheUpstreamModelsEndpoint(t *testing.T) {
	var path string
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		path = r.URL.Path
		_, _ = w.Write([]byte(`{"data":[]}`))
	}))
	defer upstream.Close()

	if err := probeUpstream(context.Background(), upstream.URL); err != nil {
		t.Fatalf("probeUpstream: %v", err)
	}
	if path != "/v1/models" {
		t.Fatalf("probed %q, want /v1/models", path)
	}
}

func TestHealthcheckFailsWhenUpstreamReturnsError(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "nope", http.StatusServiceUnavailable)
	}))
	defer upstream.Close()
	if err := probeUpstream(context.Background(), upstream.URL); err == nil {
		t.Fatal("probeUpstream succeeded against a failing upstream")
	}
}

func TestHealthcheckFailsWhenUpstreamIsDown(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	address := upstream.URL
	upstream.Close()
	if err := probeUpstream(context.Background(), address); err == nil {
		t.Fatal("probeUpstream succeeded against a closed listener")
	}
}

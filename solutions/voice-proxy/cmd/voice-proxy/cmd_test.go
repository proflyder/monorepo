package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/config"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/testfake"
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

// TestHealthcheckProbesTheUpstreamModelsEndpoint: the probe uses /v1/models.
func TestHealthcheckProbesTheUpstreamModelsEndpoint(t *testing.T) {
	fake := testfake.NewHermes()
	fake.SetResponse(http.StatusOK, `{"data":[]}`)
	upstream := testfake.Serve(t, fake)

	if err := probeUpstream(context.Background(), upstream.URL); err != nil {
		t.Fatalf("probeUpstream: %v", err)
	}
	calls := fake.Calls()
	if len(calls) != 1 || calls[0].Path != "/v1/models" {
		t.Fatalf("probed %#v, want /v1/models", calls)
	}
}

func TestHealthcheckFailsWhenUpstreamReturnsError(t *testing.T) {
	fake := testfake.NewHermes()
	fake.SetResponse(http.StatusServiceUnavailable, "nope")
	upstream := testfake.Serve(t, fake)
	if err := probeUpstream(context.Background(), upstream.URL); err == nil {
		t.Fatal("probeUpstream succeeded against a failing upstream")
	}
}

func TestHealthcheckFailsWhenUpstreamIsDown(t *testing.T) {
	upstream := httptest.NewServer(testfake.NewHermes())
	address := upstream.URL
	upstream.Close()
	if err := probeUpstream(context.Background(), address); err == nil {
		t.Fatal("probeUpstream succeeded against a closed listener")
	}
}

// TestHealthcheckAcceptsAuthRequiredUpstream: 401/403 is healthy, and the probe
// sends no credentials.
func TestHealthcheckAcceptsAuthRequiredUpstream(t *testing.T) {
	for _, code := range []int{http.StatusUnauthorized, http.StatusForbidden} {
		fake := testfake.NewHermes()
		fake.SetResponse(code, "auth required")
		upstream := httptest.NewServer(fake)
		if err := probeUpstream(context.Background(), upstream.URL); err != nil {
			t.Errorf("HTTP %d: probeUpstream = %v, want healthy", code, err)
		}
		for _, call := range fake.Calls() {
			if call.Header.Get("Authorization") != "" {
				t.Error("healthcheck must not send credentials")
			}
		}
		upstream.Close()
	}
}

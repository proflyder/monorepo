package main

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// TestRunHealthcheck exercises the probe against a fake scheduler. The config
// loader requires an endpoint, a consumer id and a token; everything else is
// defaulted so the test stays focused on readiness semantics.
func TestRunHealthcheck(t *testing.T) {
	const token = "probe-token"

	env := func(extra map[string]string) func(string) string {
		base := map[string]string{
			"SCHEDULER_ENDPOINT": "",
			"SCHEDULER_TOKEN":    token,
			"BRIDGE_CONSUMER_ID": "ha-bridge-home",
			"HA_URL":             "http://192.0.2.28:8123",
		}
		return func(k string) string {
			if v, ok := extra[k]; ok {
				return v
			}
			return base[k]
		}
	}

	t.Run("ready", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if got := r.Header.Get("Authorization"); got != "Bearer "+token {
				t.Errorf("authorization = %q", got)
			}
			w.Write([]byte(`{"ready":true}`))
		}))
		defer srv.Close()
		var out bytes.Buffer
		code := runHealthcheck(env(map[string]string{"SCHEDULER_ENDPOINT": srv.URL}), &out)
		if code != 0 {
			t.Fatalf("code = %d, output = %s", code, out.String())
		}
	})

	t.Run("not ready", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Write([]byte(`{"ready":false}`))
		}))
		defer srv.Close()
		var out bytes.Buffer
		code := runHealthcheck(env(map[string]string{"SCHEDULER_ENDPOINT": srv.URL}), &out)
		if code == 0 {
			t.Fatal("expected a non-zero exit code")
		}
		if strings.Contains(out.String(), token) {
			t.Fatal("output leaked the token")
		}
	})
}

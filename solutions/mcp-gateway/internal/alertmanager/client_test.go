package alertmanager

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestActive(t *testing.T) {
	var gotQuery string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotQuery = r.URL.RawQuery
		_, _ = w.Write([]byte(`[{"fingerprint":"a1b2c3d4e5f6","labels":{"alertname":"RicardoEndpointDown","severity":"warning"},` +
			`"annotations":{"summary":"Гермес недоступен"},"startsAt":"2026-11-01T09:00:00Z","status":{"state":"active"}}]`))
	}))
	defer srv.Close()

	alerts, err := New(srv.URL, srv.Client()).Active(context.Background(), 10)
	if err != nil {
		t.Fatalf("Active: %v", err)
	}
	if len(alerts) != 1 {
		t.Fatalf("alerts = %d", len(alerts))
	}
	if got := alerts[0].Summary(); got != "Гермес недоступен" {
		t.Fatalf("summary = %q", got)
	}
	if alerts[0].Severity() != "warning" {
		t.Fatalf("severity = %q", alerts[0].Severity())
	}
	if alerts[0].Fingerprint != "a1b2c3d4e5f6" {
		t.Fatalf("fingerprint = %q", alerts[0].Fingerprint)
	}
	for _, want := range []string{"active=true", "silenced=false", "inhibited=false"} {
		if !contains(gotQuery, want) {
			t.Fatalf("query %q missing %q", gotQuery, want)
		}
	}
}

func TestActiveErrors(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusBadGateway)
	}))
	defer srv.Close()
	if _, err := New(srv.URL, srv.Client()).Active(context.Background(), 5); err == nil {
		t.Fatal("HTTP 502 accepted")
	}
}

func contains(haystack, needle string) bool {
	for i := 0; i+len(needle) <= len(haystack); i++ {
		if haystack[i:i+len(needle)] == needle {
			return true
		}
	}
	return false
}

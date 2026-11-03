package profile_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
)

type fakeHA struct {
	state   string
	down    atomic.Bool
	status  int
	calls   atomic.Int64
	headers []string
}

func (h *fakeHA) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	h.calls.Add(1)
	h.headers = append(h.headers, r.Header.Get("Authorization"))
	if h.down.Load() {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	if h.status != 0 {
		w.WriteHeader(h.status)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{"entity_id": "input_select.ricardo_voice_profile", "state": h.state})
}

func newReader(t *testing.T, server *httptest.Server, cache time.Duration, clock func() time.Time) *profile.Reader {
	t.Helper()
	return profile.New(profile.Config{
		URL:     server.URL,
		Token:   func() string { return "test-ha-token" },
		Entity:  "input_select.ricardo_voice_profile",
		Cache:   cache,
		Timeout: time.Second,
		Clock:   clock,
	})
}

// TestMapState mirrors profile_from_state: NFKC + casefold + whitespace
// collapsing, then the exact work/personal aliases.
func TestMapState(t *testing.T) {
	cases := map[string]string{
		"Рабочий":         profile.Work,
		"рабочий профиль": profile.Work,
		"ＷＯＲＫ":            profile.Work,
		"WORK":            profile.Work,
		" Personal ":      profile.Personal,
		"личный":          profile.Personal,
		"ЛИЧНЫЙ ПРОФИЛЬ":  profile.Personal,
		"personal\u00a0":  profile.Personal,
		"Непонятно":       "",
		"":                "",
		"рабочий, личный": "",
		"work personal":   "",
	}
	for input, want := range cases {
		got, ok := profile.MapState(input)
		if got != want {
			t.Errorf("MapState(%q) = %q (ok=%t), want %q", input, got, ok, want)
		}
		if ok != (want != "") {
			t.Errorf("MapState(%q) ok = %t", input, ok)
		}
	}
}

// TestFreshProfileIsCachedBetweenRequests mirrors
// test_fresh_profile_is_cached_between_requests.
func TestFreshProfileIsCachedBetweenRequests(t *testing.T) {
	ha := &fakeHA{state: "Личный"}
	server := httptest.NewServer(ha)
	defer server.Close()
	reader := newReader(t, server, 2*time.Second, nil)

	for i := 0; i < 2; i++ {
		got, ok := reader.Profile(context.Background())
		if !ok || got != profile.Personal {
			t.Fatalf("read %d = %q (ok=%t)", i, got, ok)
		}
	}
	if calls := ha.calls.Load(); calls != 1 {
		t.Fatalf("HA calls = %d, want 1", calls)
	}
	if ha.headers[0] != "Bearer test-ha-token" {
		t.Fatalf("Authorization = %q", ha.headers[0])
	}
}

// TestStaleCacheIsNeverUsedAfterAFailedRead mirrors
// test_stale_cache_is_never_used_after_a_failed_read: the failed read clears the
// cache instead of reusing the previous profile.
func TestStaleCacheIsNeverUsedAfterAFailedRead(t *testing.T) {
	ha := &fakeHA{state: "Рабочий"}
	server := httptest.NewServer(ha)
	defer server.Close()
	reader := newReader(t, server, 0, nil)

	if got, ok := reader.Profile(context.Background()); !ok || got != profile.Work {
		t.Fatalf("first read = %q (ok=%t)", got, ok)
	}
	ha.down.Store(true)
	if got, ok := reader.Profile(context.Background()); ok {
		t.Fatalf("failed read = %q, want unknown", got)
	}
	ha.down.Store(false)
	ha.state = "Личный"
	if got, ok := reader.Profile(context.Background()); !ok || got != profile.Personal {
		t.Fatalf("recovered read = %q (ok=%t)", got, ok)
	}
}

// TestCacheExpiresAfterTheConfiguredTTL pins the 2 s window.
func TestCacheExpiresAfterTheConfiguredTTL(t *testing.T) {
	ha := &fakeHA{state: "Личный"}
	server := httptest.NewServer(ha)
	defer server.Close()
	now := time.Unix(1_700_000_000, 0)
	clock := func() time.Time { return now }
	reader := newReader(t, server, 2*time.Second, clock)

	if _, ok := reader.Profile(context.Background()); !ok {
		t.Fatal("first read failed")
	}
	now = now.Add(1900 * time.Millisecond)
	if _, ok := reader.Profile(context.Background()); !ok {
		t.Fatal("second read failed")
	}
	if calls := ha.calls.Load(); calls != 1 {
		t.Fatalf("HA calls inside the TTL = %d, want 1", calls)
	}
	now = now.Add(200 * time.Millisecond)
	ha.state = "Рабочий"
	if got, ok := reader.Profile(context.Background()); !ok || got != profile.Work {
		t.Fatalf("read after expiry = %q (ok=%t)", got, ok)
	}
	if calls := ha.calls.Load(); calls != 2 {
		t.Fatalf("HA calls = %d, want 2", calls)
	}
}

// TestUnknownStateIsNotCached: only a successful mapped read is cached.
func TestUnknownStateIsNotCached(t *testing.T) {
	ha := &fakeHA{state: "Непонятно"}
	server := httptest.NewServer(ha)
	defer server.Close()
	reader := newReader(t, server, 2*time.Second, nil)

	if got, ok := reader.Profile(context.Background()); ok {
		t.Fatalf("unknown state = %q", got)
	}
	ha.state = "Личный"
	if got, ok := reader.Profile(context.Background()); !ok || got != profile.Personal {
		t.Fatalf("second read = %q (ok=%t)", got, ok)
	}
	if calls := ha.calls.Load(); calls != 2 {
		t.Fatalf("HA calls = %d, want 2 (an unknown state must not be cached)", calls)
	}
}

// TestFetchBypassesTheCache pins the fresh read used by the updates poller and
// the queue consumer.
func TestFetchBypassesTheCache(t *testing.T) {
	ha := &fakeHA{state: "Рабочий"}
	server := httptest.NewServer(ha)
	defer server.Close()
	reader := newReader(t, server, 2*time.Second, nil)

	_, _ = reader.Fetch(context.Background())
	_, _ = reader.Fetch(context.Background())
	if calls := ha.calls.Load(); calls != 2 {
		t.Fatalf("HA calls = %d, want 2", calls)
	}
}

// TestStateTreatsErrorsAndOddBodiesAsUnknown mirrors read_entity_state.
func TestStateTreatsErrorsAndOddBodiesAsUnknown(t *testing.T) {
	for name, handler := range map[string]http.HandlerFunc{
		"error": func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusServiceUnavailable) },
		"array": func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(`[1,2]`)) },
		"null":  func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(`null`)) },
		"no-state": func(w http.ResponseWriter, r *http.Request) {
			_, _ = w.Write([]byte(`{"entity_id":"x"}`))
		},
		"non-string": func(w http.ResponseWriter, r *http.Request) {
			_, _ = w.Write([]byte(`{"state":5}`))
		},
	} {
		t.Run(name, func(t *testing.T) {
			server := httptest.NewServer(handler)
			defer server.Close()
			reader := newReader(t, server, time.Second, nil)
			if state, ok := reader.State(context.Background(), "media_player.x"); ok {
				t.Fatalf("state = %q", state)
			}
		})
	}
}

package catalogapi

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/discovery"
)

type fakeSource struct {
	snap discovery.Snapshot
	err  error
}

func (f fakeSource) Get(context.Context) (discovery.Snapshot, error) { return f.snap, f.err }

func source() fakeSource {
	entries := catalog.Build([]catalog.Raw{
		{Entity: "scene.focus", State: "x", Name: "Focus", Icon: "mdi:laptop"},
		{Entity: "light.lamp", State: "on", Name: "Lamp", Room: "Bedroom"},
		{Entity: "switch.plug", State: "unavailable", Name: "Plug"},
	}, catalog.Rules{})
	return fakeSource{snap: discovery.Snapshot{Catalog: catalog.New(entries), StatesOK: true}}
}

func get(h http.Handler, auth string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(http.MethodGet, "/v1/catalog", nil)
	if auth != "" {
		req.Header.Set("Authorization", auth)
	}
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func TestRequiresBearer(t *testing.T) {
	h := Handler(source(), "secret", nil)
	for _, auth := range []string{"", "Bearer wrong", "secret", "Basic secret"} {
		if rec := get(h, auth); rec.Code != http.StatusUnauthorized {
			t.Errorf("auth %q => %d", auth, rec.Code)
		}
	}
	if rec := get(Handler(source(), "", nil), "Bearer "); rec.Code != http.StatusUnauthorized {
		t.Errorf("empty configured token must never authorize, got %d", rec.Code)
	}
}

func TestServesDiscoveredEntries(t *testing.T) {
	rec := get(Handler(source(), "secret", nil), "Bearer secret")
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d", rec.Code)
	}
	var resp Response
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if !resp.StatesOK || len(resp.Entries) != 3 {
		t.Fatalf("resp = %+v", resp)
	}
	focus, lamp, plug := resp.Entries[0], resp.Entries[1], resp.Entries[2]
	if focus.ID != "scene.focus" || focus.Kind != "scenario" || focus.Icon != "laptop" || focus.Actions[0] != "run" || focus.State != "" {
		t.Errorf("scenario view = %+v", focus)
	}
	if lamp.State != "on" || lamp.Room != "Bedroom" || !lamp.Available || len(lamp.Actions) != 2 {
		t.Errorf("light view = %+v", lamp)
	}
	if plug.Available || plug.State != "" {
		t.Errorf("unavailable device: %+v", plug)
	}
}

func TestStaleAndUnreachable(t *testing.T) {
	stale := source()
	stale.snap.StatesOK = false
	var resp Response
	rec := get(Handler(stale, "s", nil), "Bearer s")
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil || rec.Code != http.StatusOK || resp.StatesOK {
		t.Fatalf("code=%d resp=%+v err=%v", rec.Code, resp, err)
	}
	if rec := get(Handler(fakeSource{err: errors.New("down")}, "s", nil), "Bearer s"); rec.Code != http.StatusBadGateway {
		t.Fatalf("no catalog at all must be 502, got %d", rec.Code)
	}
}

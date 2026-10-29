package catalogapi

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
)

type fakeStates struct {
	m   map[string]string
	err error
}

func (f fakeStates) States(context.Context) (map[string]string, error) { return f.m, f.err }

func testCatalog(t *testing.T) *catalog.Catalog {
	t.Helper()
	c, err := catalog.Parse([]byte(`{"version":1,"entries":[
	 {"id":"focus","kind":"scenario","title":"Focus","entity":"script.focus","icon":"work"},
	 {"id":"lamp","kind":"light","title":"Lamp","room":"Bedroom","entity":"light.lamp"},
	 {"id":"plug","kind":"switch","title":"Plug","entity":"switch.plug"}]}`))
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func get(t *testing.T, h http.Handler, auth string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, "/v1/catalog", nil)
	if auth != "" {
		req.Header.Set("Authorization", auth)
	}
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func TestRequiresBearer(t *testing.T) {
	h := Handler(testCatalog(t), fakeStates{}, "secret", nil)
	for _, auth := range []string{"", "Bearer wrong", "secret", "Basic secret"} {
		if rec := get(t, h, auth); rec.Code != http.StatusUnauthorized {
			t.Errorf("auth %q => %d", auth, rec.Code)
		}
	}
	if rec := get(t, Handler(testCatalog(t), fakeStates{}, "", nil), "Bearer "); rec.Code != http.StatusUnauthorized {
		t.Errorf("empty configured token must never authorize, got %d", rec.Code)
	}
}

func TestMergesLiveState(t *testing.T) {
	states := fakeStates{m: map[string]string{
		"script.focus": "off", "light.lamp": "on", "switch.plug": "unavailable",
	}}
	rec := get(t, Handler(testCatalog(t), states, "secret", nil), "Bearer secret")
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
	if !focus.Available || focus.State != "" || focus.Actions[0] != "run" {
		t.Errorf("scenario view = %+v", focus)
	}
	if !lamp.Available || lamp.State != "on" || lamp.Room != "Bedroom" || len(lamp.Actions) != 2 {
		t.Errorf("light view = %+v", lamp)
	}
	if plug.Available || plug.State != "" {
		t.Errorf("unavailable device must not report a state: %+v", plug)
	}
}

func TestHAUnreachableStillListsEntries(t *testing.T) {
	rec := get(t, Handler(testCatalog(t), fakeStates{err: errors.New("down")}, "s", nil), "Bearer s")
	var resp Response
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if rec.Code != http.StatusOK || resp.StatesOK || len(resp.Entries) != 3 || resp.Entries[1].Available {
		t.Fatalf("code=%d resp=%+v", rec.Code, resp)
	}
}

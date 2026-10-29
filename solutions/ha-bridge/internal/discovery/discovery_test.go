package discovery

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/ha"
)

type fakeHA struct {
	entities    []ha.Entity
	entitiesErr error
	rooms       string
	roomsErr    error
	hidden      string
	hiddenErr   error
	label       string
	labelErr    error
	calls       int
}

func (f *fakeHA) Entities(context.Context) ([]ha.Entity, error) {
	f.calls++
	return f.entities, f.entitiesErr
}

func (f *fakeHA) RenderTemplate(_ context.Context, t string) (string, error) {
	switch {
	case strings.Contains(t, "area_name"):
		return f.rooms, f.roomsErr
	case strings.Contains(t, "is_hidden_entity"):
		return f.hidden, f.hiddenErr
	case strings.Contains(t, "label_entities"):
		return f.label, f.labelErr
	}
	return "", errors.New("unexpected template")
}

func base() *fakeHA {
	return &fakeHA{
		entities: []ha.Entity{
			{ID: "light.lamp", State: "on", Name: "Lamp", Icon: "lightbulb"},
			{ID: "light.noise", State: "on", Name: "Noise"},
			{ID: "script.work_mode", State: "off", Name: "Work mode"},
			{ID: "scene.movie", State: "scening", Name: "Movie"},
		},
		rooms:  "light.lamp|Bedroom\nlight.noise|\nscript.work_mode|None\nscene.movie|Living\n",
		hidden: "light.noise\n",
		label:  "script.work_mode\nlight.lamp\n",
	}
}

func ids(s Snapshot) string {
	var out []string
	for _, e := range s.Catalog.Entries() {
		out = append(out, e.ID)
	}
	return strings.Join(out, ",")
}

func TestDiscoversFromHomeAssistant(t *testing.T) {
	p := New(base(), catalog.Rules{}, time.Second, nil)
	snap, err := p.Get(context.Background())
	if err != nil || !snap.StatesOK {
		t.Fatalf("snap=%+v err=%v", snap, err)
	}
	if got := ids(snap); got != "scene.movie,light.lamp" {
		t.Fatalf("got %s (scripts need a label, hidden are skipped)", got)
	}
	lamp, _ := snap.Catalog.Lookup("light.lamp")
	if lamp.Room != "Bedroom" || lamp.Title != "Lamp" || lamp.State != "on" {
		t.Fatalf("lamp = %+v", lamp)
	}
}

func TestLabelAllowlistIncludesScripts(t *testing.T) {
	p := New(base(), catalog.Rules{Label: "phone"}, time.Second, nil)
	snap, err := p.Get(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if got := ids(snap); got != "script.work_mode,light.lamp" {
		t.Fatalf("got %s", got)
	}
}

func TestLabelFailsClosed(t *testing.T) {
	f := base()
	f.labelErr = errors.New("no labels in this HA")
	if _, err := New(f, catalog.Rules{Label: "phone"}, time.Second, nil).Get(context.Background()); err == nil {
		t.Fatal("an unreadable label must not widen the catalog")
	}
}

func TestAreasAndHiddenAreBestEffort(t *testing.T) {
	f := base()
	f.roomsErr = errors.New("boom")
	f.hiddenErr = errors.New("boom")
	snap, err := New(f, catalog.Rules{}, time.Second, nil).Get(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	lamp, _ := snap.Catalog.Lookup("light.lamp")
	if lamp.Room != "" {
		t.Fatalf("room should be empty without areas: %+v", lamp)
	}
	if _, ok := snap.Catalog.Lookup("light.noise"); !ok {
		t.Fatal("without hidden flags nothing can be filtered by them")
	}
}

func TestCachesWithinTTLAndRefreshesAfter(t *testing.T) {
	f := base()
	p := New(f, catalog.Rules{}, 10*time.Second, nil)
	now := time.Unix(1000, 0)
	p.now = func() time.Time { return now }
	for i := 0; i < 3; i++ {
		if _, err := p.Get(context.Background()); err != nil {
			t.Fatal(err)
		}
	}
	if f.calls != 1 {
		t.Fatalf("HA read %d times inside the TTL", f.calls)
	}
	now = now.Add(11 * time.Second)
	f.entities = append(f.entities, ha.Entity{ID: "light.new", State: "off", Name: "New"})
	snap, _ := p.Get(context.Background())
	if _, ok := snap.Catalog.Lookup("light.new"); !ok || f.calls != 2 {
		t.Fatalf("a new HA entity must appear after refresh (calls=%d)", f.calls)
	}
}

func TestServesStaleCatalogWhenHAIsDown(t *testing.T) {
	f := base()
	p := New(f, catalog.Rules{}, time.Second, nil)
	now := time.Unix(1000, 0)
	p.now = func() time.Time { return now }
	if _, err := p.Get(context.Background()); err != nil {
		t.Fatal(err)
	}
	now = now.Add(time.Minute)
	f.entitiesErr = errors.New("down")
	snap, err := p.Get(context.Background())
	if err != nil || snap.StatesOK {
		t.Fatalf("snap=%+v err=%v", snap, err)
	}
	for _, e := range snap.Catalog.Entries() {
		if e.Available || e.State != "" {
			t.Fatalf("stale entries must be unavailable: %+v", e)
		}
	}
}

func TestFirstReadFailureIsAnError(t *testing.T) {
	f := base()
	f.entitiesErr = errors.New("down")
	if _, err := New(f, catalog.Rules{}, time.Second, nil).Get(context.Background()); err == nil {
		t.Fatal("no catalog can be invented without HA")
	}
}

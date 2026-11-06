package discovery

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/proflyder/monorepo/packages/go/ha"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/catalog"
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
	// profile is the answer to the profile label query. A nil map means "the
	// same as the scenario label", which is what a single-label HA would do.
	profile    string
	profileErr error
	labels     []string
	calls      int
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
		label := "scenario"
		if i := strings.Index(t, "label_entities('"); i >= 0 {
			label = strings.TrimPrefix(t[i:], "label_entities('")
			label = label[:strings.IndexByte(label, '\'')]
		}
		f.labels = append(f.labels, label)
		if label == "voice_profile" {
			return f.profile, f.profileErr
		}
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
			{ID: "script.cleaner", State: "off", Name: "Cleaner", Icon: "robot-vacuum"},
			{ID: "script.alarm_stop", State: "off", Name: "Alarm stop"},
			{ID: "scene.movie", State: "scening", Name: "Movie"},
			{ID: "switch.voice_profile", State: "off", Name: "Voice profile"},
		},
		rooms:   "light.lamp|Bedroom\nlight.noise|\nscript.work_mode|None\nscene.movie|Living\n",
		hidden:  "light.noise\n",
		label:   "script.work_mode\nscript.cleaner\n",
		profile: "switch.voice_profile\n",
	}
}

func rules() catalog.Rules {
	return catalog.Rules{ScenarioLabel: "scenario", ProfileLabel: "voice_profile"}
}

func ids(s Snapshot) string {
	var out []string
	for _, e := range s.Catalog.Entries() {
		out = append(out, e.ID)
	}
	return strings.Join(out, ",")
}

func TestDiscoversFromHomeAssistant(t *testing.T) {
	f := base()
	p := New(f, rules(), time.Second, nil)
	snap, err := p.Get(context.Background())
	if err != nil || !snap.StatesOK {
		t.Fatalf("snap=%+v err=%v", snap, err)
	}
	// scene + scripts that carry the scenario label (cleaner has an icon but so
	// does nothing else; alarm_stop has neither); hidden light.noise is out.
	// The profile switch carries the profile label and comes first.
	if got := ids(snap); got != "switch.voice_profile,scene.movie,script.cleaner,script.work_mode,light.lamp" {
		t.Fatalf("got %s", got)
	}
	lamp, _ := snap.Catalog.Lookup("light.lamp")
	if lamp.Room != "Bedroom" || lamp.Title != "Lamp" || lamp.State != "on" {
		t.Fatalf("lamp = %+v", lamp)
	}
	profile, _ := snap.Catalog.Lookup("switch.voice_profile")
	if profile.Kind != catalog.KindProfile || profile.State != "off" {
		t.Fatalf("profile = %+v", profile)
	}
	if strings.Join(f.labels, ",") != "scenario,voice_profile" {
		t.Fatalf("both role labels must be read once, got %v", f.labels)
	}
}

// The profile role is a convenience: losing it must not lose the switch, and it
// must not affect scripts or scenes.
func TestProfileLabelOutageKeepsTheSwitchAsADevice(t *testing.T) {
	f := base()
	f.profileErr = errors.New("no labels in this HA")
	snap, err := New(f, rules(), time.Second, nil).Get(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	profile, ok := snap.Catalog.Lookup("switch.voice_profile")
	if !ok || profile.Kind != catalog.KindSwitch {
		t.Fatalf("without the profile label the switch stays a device: %+v", profile)
	}
	if got := ids(snap); got != "scene.movie,script.cleaner,script.work_mode,light.lamp,switch.voice_profile" {
		t.Fatalf("got %s", got)
	}
}

func TestScenarioLabelOutageExposesNoScripts(t *testing.T) {
	f := base()
	f.labelErr = errors.New("no labels in this HA")
	snap, err := New(f, rules(), time.Second, nil).Get(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if got := ids(snap); got != "switch.voice_profile,scene.movie,light.lamp" {
		t.Fatalf("an unreadable scenario label must fail closed for scripts only, got %s", got)
	}
}

func TestAreasAndHiddenAreBestEffort(t *testing.T) {
	f := base()
	f.roomsErr = errors.New("boom")
	f.hiddenErr = errors.New("boom")
	snap, err := New(f, rules(), time.Second, nil).Get(context.Background())
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
	p := New(f, rules(), 10*time.Second, nil)
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
	p := New(f, rules(), time.Second, nil)
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
	if _, err := New(f, rules(), time.Second, nil).Get(context.Background()); err == nil {
		t.Fatal("no catalog can be invented without HA")
	}
}

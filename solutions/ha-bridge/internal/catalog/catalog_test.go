package catalog

import (
	"strings"
	"testing"
)

func raws() []Raw {
	return []Raw{
		{Entity: "light.bedside_lamp", State: "on", Name: "Bedside lamp", Icon: "mdi:lightbulb-outline", Room: "Bedroom"},
		{Entity: "light.hall", State: "off", Name: "Hall"},
		{Entity: "switch.desk_fan", State: "unavailable", Name: "Desk fan", Room: "Workspace"},
		{Entity: "scene.movie", State: "2026-01-01T00:00:00+00:00", Name: "Movie", Room: "Living"},
		{Entity: "script.work_mode", State: "off", Name: "Work mode", Labeled: true},
		{Entity: "script.alarm_start", State: "off", Name: "Alarm start"},
		{Entity: "light.secret", State: "on", Name: "Secret", Hidden: true},
		{Entity: "sensor.temp", State: "21"},
		{Entity: "switch.router_child_lock", State: "off"},
		{Entity: "Light.Bad", State: "on"},
	}
}

func ids(entries []Entry) []string {
	out := make([]string, len(entries))
	for i, e := range entries {
		out[i] = e.ID
	}
	return out
}

func TestBuildDefaultRules(t *testing.T) {
	got := ids(Build(raws(), Rules{Exclude: []string{"switch.*_child_lock"}}))
	want := []string{
		// scenarios first; scripts are NOT auto-exposed without a label
		"scene.movie",
		// devices: by room, unassigned last
		"light.bedside_lamp", "switch.desk_fan", "light.hall", // Bedroom, Workspace, (none)
	}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("got %v want %v", got, want)
	}
}

func TestBuildLabelIsAnExplicitAllowlist(t *testing.T) {
	got := ids(Build(raws(), Rules{Label: "phone"}))
	if len(got) != 1 || got[0] != "script.work_mode" {
		t.Fatalf("label must expose only labeled entities (incl. scripts), got %v", got)
	}
}

func TestBuildMapsFields(t *testing.T) {
	entries := Build(raws(), Rules{})
	byID := map[string]Entry{}
	for _, e := range entries {
		byID[e.ID] = e
	}
	lamp := byID["light.bedside_lamp"]
	if lamp.Kind != KindLight || lamp.Title != "Bedside lamp" || lamp.Room != "Bedroom" ||
		lamp.Icon != "lightbulb-outline" || lamp.State != "on" || !lamp.Available {
		t.Fatalf("lamp = %+v", lamp)
	}
	if fan := byID["switch.desk_fan"]; fan.Available || fan.State != "" {
		t.Fatalf("unavailable device must not report state: %+v", fan)
	}
	if scene := byID["scene.movie"]; scene.Kind != KindScenario || scene.State != "" || !scene.Available {
		t.Fatalf("scene = %+v", scene)
	}
	if _, ok := byID["light.secret"]; ok {
		t.Fatal("hidden entity must be skipped")
	}
	if _, ok := byID["sensor.temp"]; ok {
		t.Fatal("unsupported domain must be skipped")
	}
}

func TestTitleFallsBackToEntityId(t *testing.T) {
	e := Build([]Raw{{Entity: "light.reading_corner", State: "on"}}, Rules{})
	if len(e) != 1 || e[0].Title != "Reading corner" {
		t.Fatalf("%+v", e)
	}
}

func TestRulesValidate(t *testing.T) {
	if err := (Rules{Label: "phone", Exclude: []string{"switch.*"}}).Validate(); err != nil {
		t.Fatal(err)
	}
	for _, bad := range []Rules{{Label: "Phone App"}, {Label: "a'b"}, {Exclude: []string{"["}}} {
		if bad.Validate() == nil {
			t.Errorf("%+v must be rejected", bad)
		}
	}
}

func TestResolve(t *testing.T) {
	c := New(Build(raws(), Rules{}))
	scene, _ := c.Lookup("scene.movie")
	lamp, _ := c.Lookup("light.bedside_lamp")

	call, err := scene.Resolve(ActionRun)
	if err != nil || call != (ServiceCall{Domain: "scene", Service: "turn_on", Entity: "scene.movie"}) {
		t.Fatalf("scene run = %+v, %v", call, err)
	}
	call, err = lamp.Resolve(ActionTurnOff)
	if err != nil || call != (ServiceCall{Domain: "light", Service: "turn_off", Entity: "light.bedside_lamp"}) {
		t.Fatalf("light off = %+v, %v", call, err)
	}
	for _, bad := range []struct {
		e Entry
		a string
	}{{scene, ActionTurnOn}, {lamp, ActionRun}, {lamp, "toggle"}, {lamp, ""}} {
		if _, err := bad.e.Resolve(bad.a); err == nil || !strings.Contains(err.Error(), "not supported") {
			t.Errorf("Resolve(%s,%q) must be rejected, got %v", bad.e.ID, bad.a, err)
		}
	}
	if _, ok := c.Lookup("script.alarm_start"); ok {
		t.Fatal("non-exposed entity must not be resolvable")
	}
}

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
		{Entity: "script.work_mode", State: "off", Name: "Work mode", Icon: "mdi:laptop"},
		{Entity: "script.blackout", State: "off", Name: "Blackout", Scenario: true},
		{Entity: "script.alarm_start", State: "off", Name: "Alarm start", Icon: "mdi:alarm"},
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

func TestBuildExposesDevicesScenesAndLabeledScriptsOnly(t *testing.T) {
	got := ids(Build(raws(), Rules{ScenarioLabel: "scenario", Exclude: []string{"switch.*_child_lock"}}))
	want := []string{
		// scenarios first: scene.* and scripts carrying the scenario label
		"scene.movie", "script.blackout",
		// devices: by room, unassigned last
		"light.bedside_lamp", "switch.desk_fan", "light.hall", // Bedroom, Workspace, (none)
	}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("got %v want %v", got, want)
	}
}

// An icon is display only: it must never promote a script into the catalog.
func TestIconNeverDecidesExposure(t *testing.T) {
	got := ids(Build([]Raw{
		{Entity: "script.work_mode", State: "off", Icon: "mdi:laptop"},
		{Entity: "script.alarm_start", State: "off", Icon: "mdi:alarm"},
	}, Rules{ScenarioLabel: "scenario"}))
	if len(got) != 0 {
		t.Fatalf("scripts without the scenario label must stay out, got %v", got)
	}
}

func TestScenarioLabelDoesNotRestrictDevicesOrScenes(t *testing.T) {
	got := ids(Build([]Raw{
		{Entity: "light.a", State: "on"},
		{Entity: "scene.b", State: "x"},
		{Entity: "script.c", State: "off", Scenario: true},
		{Entity: "script.d", State: "off"},
	}, Rules{ScenarioLabel: "scenario"}))
	if strings.Join(got, ",") != "scene.b,script.c,light.a" {
		t.Fatalf("got %v", got)
	}
}

func TestBuildMapsFields(t *testing.T) {
	entries := Build(raws(), Rules{ScenarioLabel: "scenario"})
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
	e := Build([]Raw{{Entity: "light.reading_corner", State: "on"}}, Rules{ScenarioLabel: "scenario"})
	if len(e) != 1 || e[0].Title != "Reading corner" {
		t.Fatalf("%+v", e)
	}
}

func TestRulesValidate(t *testing.T) {
	if err := (Rules{ScenarioLabel: "scenario", Exclude: []string{"switch.*"}}).Validate(); err != nil {
		t.Fatal(err)
	}
	for _, bad := range []Rules{{}, {ScenarioLabel: "Phone App"}, {ScenarioLabel: "a'b"}, {ScenarioLabel: "ok", Exclude: []string{"["}}} {
		if bad.Validate() == nil {
			t.Errorf("%+v must be rejected", bad)
		}
	}
}

func TestResolve(t *testing.T) {
	c := New(Build(raws(), Rules{ScenarioLabel: "scenario"}))
	scene, _ := c.Lookup("scene.movie")
	lamp, _ := c.Lookup("light.bedside_lamp")

	call, err := scene.Resolve(ActionRun, Params{})
	if err != nil || call.Domain != "scene" || call.Service != "turn_on" || call.Entity != "scene.movie" || call.Data != nil {
		t.Fatalf("scene run = %+v, %v", call, err)
	}
	call, err = lamp.Resolve(ActionTurnOff, Params{})
	if err != nil || call.Domain != "light" || call.Service != "turn_off" || call.Entity != "light.bedside_lamp" || call.Data != nil {
		t.Fatalf("light off = %+v, %v", call, err)
	}
	for _, bad := range []struct {
		e Entry
		a string
	}{{scene, ActionTurnOn}, {lamp, ActionRun}, {lamp, "toggle"}, {lamp, ""}} {
		if _, err := bad.e.Resolve(bad.a, Params{}); err == nil || !strings.Contains(err.Error(), "not supported") {
			t.Errorf("Resolve(%s,%q) must be rejected, got %v", bad.e.ID, bad.a, err)
		}
	}
	if _, ok := c.Lookup("script.alarm_start"); ok {
		t.Fatal("non-exposed entity must not be resolvable")
	}
}

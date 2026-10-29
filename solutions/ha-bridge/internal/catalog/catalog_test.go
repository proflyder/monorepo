package catalog

import (
	"strings"
	"testing"
)

const valid = `{"version":1,"entries":[
 {"id":"focus","kind":"scenario","title":"Focus","entity":"script.focus","icon":"work"},
 {"id":"lamp","kind":"light","title":"Lamp","room":"Bedroom","entity":"light.lamp"},
 {"id":"plug","kind":"switch","title":"Plug","entity":"switch.plug"}]}`

func TestParseValid(t *testing.T) {
	c, err := Parse([]byte(valid))
	if err != nil {
		t.Fatal(err)
	}
	if got := len(c.Entries()); got != 3 {
		t.Fatalf("entries = %d", got)
	}
	if c.Entries()[0].ID != "focus" {
		t.Fatal("file order must be preserved")
	}
	if _, ok := c.Lookup("lamp"); !ok {
		t.Fatal("lookup failed")
	}
	if _, ok := c.Lookup("nope"); ok {
		t.Fatal("unknown id found")
	}
}

func TestParseRejects(t *testing.T) {
	cases := map[string]string{
		"bad json":         `{`,
		"unknown field":    `{"version":1,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"script.a","service":"x"}]}`,
		"version":          `{"version":2,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"script.a"}]}`,
		"empty":            `{"version":1,"entries":[]}`,
		"bad id":           `{"version":1,"entries":[{"id":"A b","kind":"scenario","title":"A","entity":"script.a"}]}`,
		"empty title":      `{"version":1,"entries":[{"id":"a","kind":"scenario","title":" ","entity":"script.a"}]}`,
		"wrong domain":     `{"version":1,"entries":[{"id":"a","kind":"light","title":"A","entity":"switch.a"}]}`,
		"scenario domain":  `{"version":1,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"light.a"}]}`,
		"unknown kind":     `{"version":1,"entries":[{"id":"a","kind":"lock","title":"A","entity":"lock.a"}]}`,
		"bad entity":       `{"version":1,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"script"}]}`,
		"duplicate id":     `{"version":1,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"script.a"},{"id":"a","kind":"scenario","title":"B","entity":"script.b"}]}`,
		"duplicate entity": `{"version":1,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"script.a"},{"id":"b","kind":"scenario","title":"B","entity":"script.a"}]}`,
		"bad icon":         `{"version":1,"entries":[{"id":"a","kind":"scenario","title":"A","entity":"script.a","icon":"No Way"}]}`,
	}
	for name, raw := range cases {
		if _, err := Parse([]byte(raw)); err == nil {
			t.Errorf("%s: expected an error", name)
		}
	}
}

func TestResolve(t *testing.T) {
	c, _ := Parse([]byte(valid))
	focus, _ := c.Lookup("focus")
	lamp, _ := c.Lookup("lamp")

	call, err := focus.Resolve(ActionRun)
	if err != nil || call != (ServiceCall{Domain: "script", Service: "turn_on", Entity: "script.focus"}) {
		t.Fatalf("scenario run = %+v, %v", call, err)
	}
	call, err = lamp.Resolve(ActionTurnOff)
	if err != nil || call != (ServiceCall{Domain: "light", Service: "turn_off", Entity: "light.lamp"}) {
		t.Fatalf("light off = %+v, %v", call, err)
	}
	for _, bad := range []struct {
		e Entry
		a string
	}{{focus, ActionTurnOn}, {lamp, ActionRun}, {lamp, "toggle"}, {lamp, ""}} {
		if _, err := bad.e.Resolve(bad.a); err == nil || !strings.Contains(err.Error(), "not supported") {
			t.Errorf("Resolve(%s,%q) must be rejected, got %v", bad.e.ID, bad.a, err)
		}
	}
}

package main

import "testing"

func env(m map[string]string) func(string) string { return func(k string) string { return m[k] } }

func base() map[string]string {
	return map[string]string{
		"HA_URL": "http://192.0.2.28:8123", "HA_TOKEN": "h",
		"SCHEDULER_ENDPOINT": "http://192.0.2.27:8098/ricardo.v1.SchedulerService", "SCHEDULER_TOKEN": "s",
		"CONTROL_TOKEN": "c", "CONTROL_CATALOG": "/etc/catalog.json", "CONTROL_CONSUMER_ID": "ha-control",
	}
}

func TestLoadSettingsDefaults(t *testing.T) {
	s, err := loadSettings(env(base()))
	if err != nil {
		t.Fatal(err)
	}
	if s.Queue != "control" || s.Listen != "127.0.0.1:8099" || s.Execute {
		t.Fatalf("defaults = %+v", s)
	}
	if s.SchedulerURL != "http://192.0.2.27:8098" {
		t.Fatalf("scheduler url = %q", s.SchedulerURL)
	}
}

func TestLoadSettingsRejects(t *testing.T) {
	mutate := map[string]func(map[string]string){
		"no control token": func(m map[string]string) { delete(m, "CONTROL_TOKEN") },
		"no catalog":       func(m map[string]string) { delete(m, "CONTROL_CATALOG") },
		"home queue":       func(m map[string]string) { m["CONTROL_QUEUE"] = "home" },
		"tasks queue":      func(m map[string]string) { m["CONTROL_QUEUE"] = "tasks" },
		"bad consumer":     func(m map[string]string) { m["CONTROL_CONSUMER_ID"] = "a b" },
		"creds in url":     func(m map[string]string) { m["HA_URL"] = "http://u:p@host" },
		"bad listen":       func(m map[string]string) { m["CONTROL_LISTEN"] = "8099" },
		"bad execute":      func(m map[string]string) { m["CONTROL_EXECUTE"] = "maybe" },
	}
	for name, f := range mutate {
		m := base()
		f(m)
		if _, err := loadSettings(env(m)); err == nil {
			t.Errorf("%s: expected an error", name)
		}
	}
}

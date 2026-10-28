package config

import (
	"strings"
	"testing"
	"time"
)

func env(values map[string]string) func(string) string {
	return func(key string) string { return values[key] }
}

func noFile(string) ([]byte, error) { return nil, nil }

func TestDefaultsAndToken(t *testing.T) {
	c, err := load(env(map[string]string{"SCHEDULER_TOKEN": "secret"}), noFile)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if c.Listen != "127.0.0.1:8098" || c.DBPath != "scheduler.db" || c.Timezone != "UTC" {
		t.Fatalf("defaults = %+v", c)
	}
	if c.MissedGrace != 600*time.Second || c.RingWindow != 900*time.Second {
		t.Fatalf("policy defaults = %v/%v", c.MissedGrace, c.RingWindow)
	}
	if c.Token != "secret" {
		t.Fatalf("token not loaded")
	}
}

func TestTokenRequired(t *testing.T) {
	if _, err := load(env(nil), noFile); err == nil || !strings.Contains(err.Error(), "SCHEDULER_TOKEN") {
		t.Fatalf("missing token error = %v", err)
	}
}

func TestTokenFile(t *testing.T) {
	c, err := load(env(map[string]string{"SCHEDULER_TOKEN_FILE": "/run/token"}), func(path string) ([]byte, error) {
		if path != "/run/token" {
			t.Fatalf("unexpected token path %q", path)
		}
		return []byte("  from-file\n"), nil
	})
	if err != nil || c.Token != "from-file" {
		t.Fatalf("token file = %q, %v", c.Token, err)
	}
}

func TestLegacyAlarmFallbacksAndPrecedence(t *testing.T) {
	c, err := load(env(map[string]string{
		"SCHEDULER_TOKEN":   "t",
		"ALARM_DB_PATH":     "/legacy/alarm.db",
		"ALARM_LISTEN":      "127.0.0.1:9000",
		"ALARM_TIMEZONE":    "Asia/Almaty",
		"SCHEDULER_DB_PATH": "/new/scheduler.db",
	}), noFile)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if c.DBPath != "/new/scheduler.db" {
		t.Fatalf("db path = %q, want SCHEDULER_DB_PATH precedence", c.DBPath)
	}
	if c.Listen != "127.0.0.1:9000" || c.Timezone != "Asia/Almaty" {
		t.Fatalf("fallbacks = %+v", c)
	}
}

func TestConsumersAndBounds(t *testing.T) {
	c, err := load(env(map[string]string{
		"SCHEDULER_TOKEN":      "t",
		"SCHEDULER_CONSUMERS":  "home-1, android, ,",
		"SCHEDULER_MAX_TIMERS": "8",
	}), noFile)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if len(c.AllowedConsumers) != 2 || c.AllowedConsumers[1] != "android" {
		t.Fatalf("consumers = %+v", c.AllowedConsumers)
	}
	if c.MaxTimers != 8 {
		t.Fatalf("max timers = %d", c.MaxTimers)
	}
}

func TestInvalidConfiguration(t *testing.T) {
	cases := map[string]map[string]string{
		"bad timezone":      {"SCHEDULER_TOKEN": "t", "SCHEDULER_TIMEZONE": "Mars/Olympus"},
		"negative grace":    {"SCHEDULER_TOKEN": "t", "SCHEDULER_MISSED_GRACE_SECONDS": "-5"},
		"huge ring window":  {"SCHEDULER_TOKEN": "t", "SCHEDULER_RING_WINDOW_SECONDS": "999999"},
		"bad tick":          {"SCHEDULER_TOKEN": "t", "SCHEDULER_TICK_MS": "10"},
		"bad int":           {"SCHEDULER_TOKEN": "t", "SCHEDULER_MAX_TIMERS": "many"},
		"huge timers":       {"SCHEDULER_TOKEN": "t", "SCHEDULER_MAX_TIMERS": "100000"},
		"huge timer limit":  {"SCHEDULER_TOKEN": "t", "SCHEDULER_MAX_TIMER_SECONDS": "99999999"},
		"bad payload limit": {"SCHEDULER_TOKEN": "t", "SCHEDULER_MAX_PAYLOAD_BYTES": "10"},
	}
	for name, values := range cases {
		t.Run(name, func(t *testing.T) {
			if _, err := load(env(values), noFile); err == nil {
				t.Fatalf("configuration accepted")
			}
		})
	}
}

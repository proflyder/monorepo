package config

import (
	"strings"
	"testing"
)

func env(values map[string]string) func(string) string {
	return func(key string) string { return values[key] }
}

func TestLoadRequiredAndDefaults(t *testing.T) {
	cfg, err := load(env(map[string]string{
		"BOT_TOKEN":      "token",
		"CHAT_ID":        "123, -100999",
		"WEBHOOK_SECRET": "secret",
		"API_TOKEN":      "api",
	}))
	if err != nil {
		t.Fatal(err)
	}
	if len(cfg.AllowedChats) != 2 || cfg.AllowedChats[0] != "123" || cfg.AllowedChats[1] != "-100999" {
		t.Fatalf("chats = %v", cfg.AllowedChats)
	}
	if cfg.DBPath != DefaultDBPath || cfg.HTTPAddr != DefaultHTTPAddr || cfg.WebhookAddr != DefaultWebhookAddr {
		t.Fatalf("defaults = %+v", cfg)
	}
	if cfg.Location.String() != "UTC" || cfg.HourlyMinute != 0 || cfg.RetentionDays != 35 {
		t.Fatalf("schedule defaults = %+v", cfg)
	}
	if cfg.CleanupDay.String() != "Sunday" || cfg.CleanupHour != 0 {
		t.Fatalf("cleanup defaults = %+v", cfg)
	}
}

func TestLoadFailsFastOnMissingRequired(t *testing.T) {
	_, err := load(env(map[string]string{"BOT_TOKEN": "token"}))
	if err == nil {
		t.Fatal("expected an error")
	}
	for _, name := range []string{"CHAT_ID", "WEBHOOK_SECRET", "API_TOKEN"} {
		if !strings.Contains(err.Error(), name) {
			t.Fatalf("error %q should mention %s", err, name)
		}
	}
}

func TestLoadOverridesAndValidation(t *testing.T) {
	cfg, err := load(env(map[string]string{
		"BOT_TOKEN":       "token",
		"CHAT_ID":         "1",
		"WEBHOOK_SECRET":  "secret",
		"API_TOKEN":       "api",
		"TZ":              "Asia/Almaty",
		"HOURLY_MINUTE":   "15",
		"CLEANUP_WEEKDAY": "mon",
		"CLEANUP_HOUR":    "3",
	}))
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Timezone != "Asia/Almaty" || cfg.HourlyMinute != 15 || cfg.CleanupHour != 3 || cfg.CleanupDay.String() != "Monday" {
		t.Fatalf("overrides = %+v", cfg)
	}

	if _, err := load(env(map[string]string{
		"BOT_TOKEN": "t", "CHAT_ID": "1", "WEBHOOK_SECRET": "s", "API_TOKEN": "a", "TZ": "Nowhere/Nope",
	})); err == nil {
		t.Fatal("expected invalid timezone error")
	}
	if _, err := load(env(map[string]string{
		"BOT_TOKEN": "t", "CHAT_ID": "1", "WEBHOOK_SECRET": "s", "API_TOKEN": "a", "HOURLY_MINUTE": "99",
	})); err == nil {
		t.Fatal("expected hourly minute range error")
	}
}

// Package config loads currency-bot settings from environment variables only.
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// Defaults documented in the README.
const (
	DefaultDBPath      = "/data/currency-bot.db"
	DefaultHTTPAddr    = ":8080"
	DefaultWebhookAddr = ":8081"
	DefaultTimezone    = "UTC"
	DefaultKursURL     = "https://kurs.kz"
	DefaultTelegramAPI = "https://api.telegram.org"
)

// Config is a validated process configuration. Required values are never
// empty; the process fails fast otherwise.
type Config struct {
	BotToken      string
	AllowedChats  []string
	WebhookSecret string
	APIToken      string
	WebhookURL    string

	DBPath      string
	HTTPAddr    string
	WebhookAddr string
	KursURL     string
	TelegramAPI string
	Timezone    string
	Location    *time.Location

	HourlyMinute  int
	CleanupDay    time.Weekday
	CleanupHour   int
	RetentionDays int
}

// Load reads and validates the process environment.
func Load() (Config, error) { return load(os.Getenv) }

func load(getenv func(string) string) (Config, error) {
	cfg := Config{
		BotToken:      strings.TrimSpace(getenv("BOT_TOKEN")),
		WebhookSecret: strings.TrimSpace(getenv("WEBHOOK_SECRET")),
		APIToken:      strings.TrimSpace(getenv("API_TOKEN")),
		WebhookURL:    strings.TrimSpace(getenv("WEBHOOK_URL")),
		DBPath:        valueOr(getenv, "DB_PATH", DefaultDBPath),
		HTTPAddr:      valueOr(getenv, "HTTP_ADDR", DefaultHTTPAddr),
		WebhookAddr:   valueOr(getenv, "WEBHOOK_ADDR", DefaultWebhookAddr),
		KursURL:       valueOr(getenv, "KURS_URL", DefaultKursURL),
		TelegramAPI:   valueOr(getenv, "TELEGRAM_API_URL", DefaultTelegramAPI),
		Timezone:      valueOr(getenv, "TZ", DefaultTimezone),
		HourlyMinute:  0,
		CleanupDay:    time.Sunday,
		CleanupHour:   0,
		RetentionDays: 35,
	}

	var missing []string
	for name, value := range map[string]string{
		"BOT_TOKEN":      cfg.BotToken,
		"CHAT_ID":        strings.TrimSpace(getenv("CHAT_ID")),
		"WEBHOOK_SECRET": cfg.WebhookSecret,
		"API_TOKEN":      cfg.APIToken,
	} {
		if value == "" {
			missing = append(missing, name)
		}
	}
	if len(missing) > 0 {
		return cfg, fmt.Errorf("missing required environment variables: %s", strings.Join(sorted(missing), ", "))
	}

	for _, part := range strings.Split(getenv("CHAT_ID"), ",") {
		if chat := strings.TrimSpace(part); chat != "" {
			cfg.AllowedChats = append(cfg.AllowedChats, chat)
		}
	}
	if len(cfg.AllowedChats) == 0 {
		return cfg, fmt.Errorf("CHAT_ID must contain at least one chat id")
	}

	loc, err := time.LoadLocation(cfg.Timezone)
	if err != nil {
		return cfg, fmt.Errorf("TZ: unknown IANA timezone %q", cfg.Timezone)
	}
	cfg.Location = loc

	if cfg.HourlyMinute, err = intValue(getenv, "HOURLY_MINUTE", 0, 0, 59); err != nil {
		return cfg, err
	}
	if cfg.CleanupHour, err = intValue(getenv, "CLEANUP_HOUR", 0, 0, 23); err != nil {
		return cfg, err
	}
	if raw := strings.TrimSpace(getenv("CLEANUP_WEEKDAY")); raw != "" {
		day, err := parseWeekday(raw)
		if err != nil {
			return cfg, err
		}
		cfg.CleanupDay = day
	}
	return cfg, nil
}

func valueOr(getenv func(string) string, key, def string) string {
	if v := strings.TrimSpace(getenv(key)); v != "" {
		return v
	}
	return def
}

func intValue(getenv func(string) string, key string, def, min, max int) (int, error) {
	raw := strings.TrimSpace(getenv(key))
	if raw == "" {
		return def, nil
	}
	n, err := strconv.Atoi(raw)
	if err != nil || n < min || n > max {
		return def, fmt.Errorf("%s must be an integer in %d..%d", key, min, max)
	}
	return n, nil
}

func parseWeekday(raw string) (time.Weekday, error) {
	names := map[string]time.Weekday{
		"0": time.Sunday, "sun": time.Sunday, "sunday": time.Sunday,
		"1": time.Monday, "mon": time.Monday, "monday": time.Monday,
		"2": time.Tuesday, "tue": time.Tuesday, "tuesday": time.Tuesday,
		"3": time.Wednesday, "wed": time.Wednesday, "wednesday": time.Wednesday,
		"4": time.Thursday, "thu": time.Thursday, "thursday": time.Thursday,
		"5": time.Friday, "fri": time.Friday, "friday": time.Friday,
		"6": time.Saturday, "sat": time.Saturday, "saturday": time.Saturday,
	}
	day, ok := names[strings.ToLower(strings.TrimSpace(raw))]
	if !ok {
		return 0, fmt.Errorf("CLEANUP_WEEKDAY must be 0..6 or a weekday name")
	}
	return day, nil
}

func sorted(values []string) []string {
	out := append([]string(nil), values...)
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j] < out[j-1]; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

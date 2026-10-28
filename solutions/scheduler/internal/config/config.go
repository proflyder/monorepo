// Package config loads scheduler settings from the environment. SCHEDULER_*
// names take precedence; legacy ALARM_* names remain supported as fallbacks so
// the existing deployment keeps working.
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// Defaults documented in the scheduler README.
const (
	DefaultListen           = "127.0.0.1:8098"
	DefaultDBPath           = "scheduler.db"
	DefaultTimezone         = "UTC"
	DefaultMissedGrace      = 600 * time.Second
	DefaultRingWindow       = 900 * time.Second
	DefaultRetention        = 30 * 24 * time.Hour
	DefaultMaxTimers        = 64
	DefaultMaxTimerDuration = 7 * 24 * time.Hour
	DefaultMaxPayloadBytes  = 16 * 1024
	DefaultMaxAttempts      = 5
	DefaultTickInterval     = time.Second
)

// Config is a fully validated process configuration.
type Config struct {
	DBPath           string
	Listen           string
	Timezone         string
	Token            string
	MissedGrace      time.Duration
	RingWindow       time.Duration
	EventRetention   time.Duration
	MaxTimers        int
	MaxTimerDuration time.Duration
	MaxPayloadBytes  int
	MaxAttempts      int
	TickInterval     time.Duration
	AllowedConsumers []string
}

// Load reads and validates the process environment.
func Load() (Config, error) {
	return load(os.Getenv, os.ReadFile)
}

func load(getenv func(string) string, readFile func(string) ([]byte, error)) (Config, error) {
	c := Config{
		DBPath:           first(getenv, "SCHEDULER_DB_PATH", "ALARM_DB_PATH"),
		Listen:           first(getenv, "SCHEDULER_LISTEN", "ALARM_LISTEN"),
		Timezone:         first(getenv, "SCHEDULER_TIMEZONE", "ALARM_TIMEZONE"),
		MissedGrace:      DefaultMissedGrace,
		RingWindow:       DefaultRingWindow,
		EventRetention:   DefaultRetention,
		MaxTimers:        DefaultMaxTimers,
		MaxTimerDuration: DefaultMaxTimerDuration,
		MaxPayloadBytes:  DefaultMaxPayloadBytes,
		MaxAttempts:      DefaultMaxAttempts,
		TickInterval:     DefaultTickInterval,
	}
	if c.DBPath == "" {
		c.DBPath = DefaultDBPath
	}
	if c.Listen == "" {
		c.Listen = DefaultListen
	}
	if c.Timezone == "" {
		c.Timezone = DefaultTimezone
	}
	if _, err := time.LoadLocation(c.Timezone); err != nil {
		return c, fmt.Errorf("SCHEDULER_TIMEZONE: unknown IANA timezone %q", c.Timezone)
	}

	token := strings.TrimSpace(getenv("SCHEDULER_TOKEN"))
	tokenFile := strings.TrimSpace(getenv("SCHEDULER_TOKEN_FILE"))
	if token == "" && tokenFile != "" {
		data, err := readFile(tokenFile)
		if err != nil {
			return c, fmt.Errorf("SCHEDULER_TOKEN_FILE: cannot read token file")
		}
		token = strings.TrimSpace(string(data))
	}
	if token == "" {
		return c, fmt.Errorf("SCHEDULER_TOKEN or SCHEDULER_TOKEN_FILE is required")
	}
	c.Token = token

	var err error
	if c.MissedGrace, err = durationSeconds(getenv, "SCHEDULER_MISSED_GRACE_SECONDS", c.MissedGrace); err != nil {
		return c, err
	}
	if c.RingWindow, err = durationSeconds(getenv, "SCHEDULER_RING_WINDOW_SECONDS", c.RingWindow); err != nil {
		return c, err
	}
	if c.EventRetention, err = durationSeconds(getenv, "SCHEDULER_EVENT_RETENTION_SECONDS", c.EventRetention); err != nil {
		return c, err
	}
	if c.MaxTimerDuration, err = durationSeconds(getenv, "SCHEDULER_MAX_TIMER_SECONDS", c.MaxTimerDuration); err != nil {
		return c, err
	}
	if c.TickInterval, err = durationMillis(getenv, "SCHEDULER_TICK_MS", c.TickInterval); err != nil {
		return c, err
	}
	if c.MaxTimers, err = intValue(getenv, "SCHEDULER_MAX_TIMERS", c.MaxTimers); err != nil {
		return c, err
	}
	if c.MaxPayloadBytes, err = intValue(getenv, "SCHEDULER_MAX_PAYLOAD_BYTES", c.MaxPayloadBytes); err != nil {
		return c, err
	}
	if c.MaxAttempts, err = intValue(getenv, "SCHEDULER_EVENT_MAX_ATTEMPTS", c.MaxAttempts); err != nil {
		return c, err
	}
	if raw := strings.TrimSpace(getenv("SCHEDULER_CONSUMERS")); raw != "" {
		for _, part := range strings.Split(raw, ",") {
			if p := strings.TrimSpace(part); p != "" {
				c.AllowedConsumers = append(c.AllowedConsumers, p)
			}
		}
	}
	if err := c.validate(); err != nil {
		return c, err
	}
	return c, nil
}

func (c Config) validate() error {
	if c.MissedGrace <= 0 || c.MissedGrace > 24*time.Hour {
		return fmt.Errorf("SCHEDULER_MISSED_GRACE_SECONDS must be within 1s..24h")
	}
	if c.RingWindow <= 0 || c.RingWindow > 24*time.Hour {
		return fmt.Errorf("SCHEDULER_RING_WINDOW_SECONDS must be within 1s..24h")
	}
	if c.EventRetention < time.Hour || c.EventRetention > 10*365*24*time.Hour {
		return fmt.Errorf("SCHEDULER_EVENT_RETENTION_SECONDS must be within 1h..10y")
	}
	if c.MaxTimerDuration <= 0 || c.MaxTimerDuration > 30*24*time.Hour {
		return fmt.Errorf("SCHEDULER_MAX_TIMER_SECONDS must be within 1s..30d")
	}
	if c.MaxTimers < 1 || c.MaxTimers > 4096 {
		return fmt.Errorf("SCHEDULER_MAX_TIMERS must be within 1..4096")
	}
	if c.MaxPayloadBytes < 1024 || c.MaxPayloadBytes > 1<<20 {
		return fmt.Errorf("SCHEDULER_MAX_PAYLOAD_BYTES must be within 1024..1048576")
	}
	if c.MaxAttempts < 1 || c.MaxAttempts > 100 {
		return fmt.Errorf("SCHEDULER_EVENT_MAX_ATTEMPTS must be within 1..100")
	}
	if c.TickInterval < 50*time.Millisecond || c.TickInterval > time.Minute {
		return fmt.Errorf("SCHEDULER_TICK_MS must be within 50..60000")
	}
	return nil
}

func first(getenv func(string) string, keys ...string) string {
	for _, key := range keys {
		if v := strings.TrimSpace(getenv(key)); v != "" {
			return v
		}
	}
	return ""
}

func intValue(getenv func(string) string, key string, def int) (int, error) {
	raw := strings.TrimSpace(getenv(key))
	if raw == "" {
		return def, nil
	}
	n, err := strconv.Atoi(raw)
	if err != nil {
		return def, fmt.Errorf("%s must be an integer", key)
	}
	return n, nil
}

func durationSeconds(getenv func(string) string, key string, def time.Duration) (time.Duration, error) {
	n, err := intValue(getenv, key, int(def/time.Second))
	if err != nil {
		return def, err
	}
	return time.Duration(n) * time.Second, nil
}

func durationMillis(getenv func(string) string, key string, def time.Duration) (time.Duration, error) {
	n, err := intValue(getenv, key, int(def/time.Millisecond))
	if err != nil {
		return def, err
	}
	return time.Duration(n) * time.Millisecond, nil
}

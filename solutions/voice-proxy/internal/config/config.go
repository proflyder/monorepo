// Package config loads voice-proxy settings from the environment only.
//
// The variable names, defaults and secret handling are copied from
// projects/home-assistant/hermes/voice_proxy.py so the Go binary can take over
// the Python image without a values change:
//
//   - a "*_FILE" variable wins over the inline value and is re-read from disk;
//     a missing or unreadable file yields an empty secret, never a fallback;
//   - trailing slashes are stripped from every base URL;
//   - the routing, updates and queue switches keep their Python semantics;
//   - a default applies only when the variable is unset, not when it is set to
//     an empty string (os.environ.get(name, default)).
//
// Values that belong to a project composition (entity ids, script names, the
// satellite) stay overridable there; the defaults mirror the current home
// deployment and are documented in README.md.
package config

import (
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

// Defaults from voice_proxy.py.
const (
	DefaultUpstream          = "http://127.0.0.1:8642"
	DefaultBind              = "127.0.0.1"
	DefaultPort              = 8643
	DefaultReasoningEffort   = "low"
	DefaultProfileEntity     = "input_select.ricardo_voice_profile"
	DefaultProfileCacheMS    = 2000
	DefaultHATimeout         = 3.0
	DefaultGatewayTimeout    = 5.0
	DefaultResultTimeout     = 900.0
	DefaultResultRetry       = 2.0
	DefaultResultWait        = 25
	DefaultUpdatesRetry      = 2.0
	DefaultUpdatesDisabled   = "1"
	DefaultAnnounceMaxChars  = 200
	DefaultAnnounceScript    = "script.schedule_announce"
	DefaultSatellite         = "assist_satellite.iandeks_stantsiia_midi_assist_satellite"
	DefaultMediaPlayer       = "media_player.iandeks_stantsiia_midi_media_player"
	DefaultQueueConsumer     = "voice-proxy-home"
	DefaultAnnounceTTL       = 10800
	DefaultUpdatesRescan     = 300
	DefaultQueueLeaseSeconds = 120
)

// Config is a validated process configuration.
type Config struct {
	Upstream           string
	Bind               string
	Port               int
	ReasoningEffort    string
	HAURL              string
	HAToken            string
	HATokenFile        string
	GatewayURL         string
	GatewayToken       string
	GatewayTokenFile   string
	ProfileEntity      string
	ProfileCache       time.Duration
	HATimeout          time.Duration
	GatewayTimeout     time.Duration
	ResultTimeout      time.Duration
	ResultRetry        time.Duration
	ResultWait         int
	UpdatesDisabled    bool
	UpdatesRetry       time.Duration
	AnnounceMaxChars   int
	AnnounceScript     string
	Satellite          string
	MediaPlayer        string
	SchedulerURL       string
	SchedulerToken     string
	SchedulerTokenFile string
	QueueDisabled      bool
	QueueConsumer      string
	AnnounceTTL        int
	UpdatesRescan      int

	// Secret sources are kept so a caller can re-read a mounted token on every
	// use, exactly like ha_token()/voice_token()/scheduler_token() in
	// voice_proxy.py. Python never caches a *_FILE secret.
	haTokenInline        string
	gatewayTokenInline   string
	schedulerTokenInline string
	readFile             ReadFile
}

// LookupEnv is the environment accessor used by Load; tests inject a map.
type LookupEnv func(string) (string, bool)

// ReadFile is the secret-file accessor used by Load; tests inject a stub.
type ReadFile func(string) ([]byte, error)

// Load reads and validates the process environment.
func Load() (Config, error) { return load(os.LookupEnv, os.ReadFile) }

// LoadFrom is Load with injected environment and file access, so tests cover
// defaults, secret precedence and validation without touching os.
func LoadFrom(lookup LookupEnv, readFile ReadFile) (Config, error) {
	return load(lookup, readFile)
}

func load(lookup LookupEnv, readFile ReadFile) (Config, error) {
	cfg := Config{
		Upstream:           baseURL(lookup, "HERMES_UPSTREAM", DefaultUpstream),
		Bind:               stringEnv(lookup, "HERMES_PROXY_BIND", DefaultBind),
		ReasoningEffort:    stringEnv(lookup, "HERMES_VOICE_REASONING_EFFORT", DefaultReasoningEffort),
		HAURL:              baseURL(lookup, "HERMES_HA_URL", ""),
		HATokenFile:        stringEnv(lookup, "HERMES_HA_TOKEN_FILE", ""),
		GatewayURL:         baseURL(lookup, "HERMES_VOICE_GATEWAY_URL", ""),
		GatewayTokenFile:   stringEnv(lookup, "HERMES_VOICE_GATEWAY_TOKEN_FILE", ""),
		ProfileEntity:      stringEnv(lookup, "HERMES_VOICE_PROFILE_ENTITY", DefaultProfileEntity),
		AnnounceScript:     stringEnv(lookup, "HERMES_VOICE_ANNOUNCE_SCRIPT", DefaultAnnounceScript),
		Satellite:          stringEnv(lookup, "HERMES_VOICE_SATELLITE", DefaultSatellite),
		MediaPlayer:        stringEnv(lookup, "HERMES_VOICE_MEDIA_PLAYER", DefaultMediaPlayer),
		SchedulerURL:       baseURL(lookup, "HERMES_SCHEDULER_URL", ""),
		SchedulerTokenFile: stringEnv(lookup, "HERMES_SCHEDULER_TOKEN_FILE", ""),
		QueueConsumer:      stringEnv(lookup, "HERMES_VOICE_CONSUMER", DefaultQueueConsumer),
		UpdatesDisabled:    switchedOff(stringEnv(lookup, "HERMES_VOICE_UPDATES", DefaultUpdatesDisabled)),
		QueueDisabled:      switchedOff(stringEnv(lookup, "HERMES_VOICE_QUEUE", DefaultUpdatesDisabled)),
	}

	var err error
	if cfg.Port, err = intEnv(lookup, "HERMES_PROXY_PORT", DefaultPort); err != nil {
		return cfg, err
	}
	cacheMS, err := intEnv(lookup, "HERMES_VOICE_PROFILE_CACHE_MS", DefaultProfileCacheMS)
	if err != nil {
		return cfg, err
	}
	cfg.ProfileCache = time.Duration(cacheMS) * time.Millisecond
	if cfg.ResultWait, err = intEnv(lookup, "HERMES_VOICE_RESULT_WAIT", DefaultResultWait); err != nil {
		return cfg, err
	}
	if cfg.AnnounceMaxChars, err = intEnv(lookup, "HERMES_VOICE_ANNOUNCE_MAX_CHARS", DefaultAnnounceMaxChars); err != nil {
		return cfg, err
	}
	if cfg.AnnounceTTL, err = intEnv(lookup, "HERMES_VOICE_ANNOUNCE_TTL", DefaultAnnounceTTL); err != nil {
		return cfg, err
	}
	if cfg.UpdatesRescan, err = intEnv(lookup, "HERMES_VOICE_UPDATES_RESCAN", DefaultUpdatesRescan); err != nil {
		return cfg, err
	}
	if cfg.HATimeout, err = secondsEnv(lookup, "HERMES_HA_TIMEOUT", DefaultHATimeout); err != nil {
		return cfg, err
	}
	if cfg.GatewayTimeout, err = secondsEnv(lookup, "HERMES_VOICE_GATEWAY_TIMEOUT", DefaultGatewayTimeout); err != nil {
		return cfg, err
	}
	if cfg.ResultTimeout, err = secondsEnv(lookup, "HERMES_VOICE_RESULT_TIMEOUT", DefaultResultTimeout); err != nil {
		return cfg, err
	}
	if cfg.ResultRetry, err = secondsEnv(lookup, "HERMES_VOICE_RESULT_RETRY", DefaultResultRetry); err != nil {
		return cfg, err
	}
	if cfg.UpdatesRetry, err = secondsEnv(lookup, "HERMES_VOICE_UPDATES_RETRY", DefaultUpdatesRetry); err != nil {
		return cfg, err
	}

	cfg.HAToken = readSecret(cfg.HATokenFile, stringEnv(lookup, "HERMES_HA_TOKEN", ""), readFile)
	cfg.GatewayToken = readSecret(cfg.GatewayTokenFile, stringEnv(lookup, "HERMES_VOICE_GATEWAY_TOKEN", ""), readFile)
	cfg.SchedulerToken = readSecret(cfg.SchedulerTokenFile, stringEnv(lookup, "HERMES_SCHEDULER_TOKEN", ""), readFile)
	cfg.haTokenInline = stringEnv(lookup, "HERMES_HA_TOKEN", "")
	cfg.gatewayTokenInline = stringEnv(lookup, "HERMES_VOICE_GATEWAY_TOKEN", "")
	cfg.schedulerTokenInline = stringEnv(lookup, "HERMES_SCHEDULER_TOKEN", "")
	cfg.readFile = readFile
	return cfg, nil
}

// HATokenValue returns the Home Assistant token, re-reading a mounted file on
// every call so a rotated secret is picked up without a restart.
func (c Config) HATokenValue() string {
	return readSecret(c.HATokenFile, inlineOr(c.haTokenInline, c.HAToken), c.readFile)
}

// GatewayTokenValue returns the mcp-gateway token with the same lazy file
// semantics as HATokenValue.
func (c Config) GatewayTokenValue() string {
	return readSecret(c.GatewayTokenFile, inlineOr(c.gatewayTokenInline, c.GatewayToken), c.readFile)
}

// SchedulerTokenValue returns the scheduler token with the same lazy file
// semantics as HATokenValue.
func (c Config) SchedulerTokenValue() string {
	return readSecret(c.SchedulerTokenFile, inlineOr(c.schedulerTokenInline, c.SchedulerToken), c.readFile)
}

// inlineOr keeps a hand-built Config (tests, -status) working: Load always
// records the inline environment value, so the fallback only matters when a
// caller constructed Config directly.
func inlineOr(value, fallback string) string {
	if value != "" {
		return value
	}
	return fallback
}

// Listen is the TCP address the HTTP server binds.
func (c Config) Listen() string { return net.JoinHostPort(c.Bind, strconv.Itoa(c.Port)) }

// RoutingEnabled is true only when the full profile -> gateway path is
// configured. A partially configured deployment keeps the previous sanitiser
// behaviour instead of failing at startup.
func (c Config) RoutingEnabled() bool {
	return c.GatewayURL != "" && c.GatewayTokenValue() != "" && c.HAURL != "" && c.HATokenValue() != ""
}

// UpdatesEnabled reports whether the dispatcher updates poller should run.
func (c Config) UpdatesEnabled() bool { return c.RoutingEnabled() && !c.UpdatesDisabled }

// QueueEnabled is true only when the durable announcement queue is fully
// configured; the URL and token are its own, not the gateway's.
func (c Config) QueueEnabled() bool {
	return c.SchedulerURL != "" && c.SchedulerTokenValue() != "" && !c.QueueDisabled
}

// QueueEnabledAndRouted mirrors main(): the consumer needs both switches.
func (c Config) QueueEnabledAndRouted() bool { return c.QueueEnabled() && c.RoutingEnabled() }

// QueueName is the scheduler queue for one profile.
func (c Config) QueueName(profile string) string { return "voice-" + profile }

// Redacted is the JSON-safe summary printed by -status. It never carries a
// token or any other secret, only whether one is configured.
type Redacted struct {
	Listen               string `json:"listen"`
	Upstream             string `json:"upstream"`
	ReasoningEffort      string `json:"reasoning_effort"`
	RoutingEnabled       bool   `json:"routing_enabled"`
	UpdatesEnabled       bool   `json:"updates_enabled"`
	QueueEnabled         bool   `json:"queue_enabled"`
	QueueConsumerEnabled bool   `json:"queue_consumer_enabled"`
	ProfileEntity        string `json:"profile_entity"`
	AnnounceScript       string `json:"announce_script"`
	Satellite            string `json:"satellite"`
	MediaPlayer          string `json:"media_player"`
	AnnounceMaxChars     int    `json:"announce_max_chars"`
	QueueConsumer        string `json:"queue_consumer"`
	SchedulerURL         string `json:"scheduler_url"`
	HAConfigured         bool   `json:"ha_configured"`
	GatewayConfigured    bool   `json:"gateway_configured"`
	SchedulerAuth        bool   `json:"scheduler_configured"`
	AnnounceTTL          int    `json:"announce_ttl"`
	UpdatesRescan        int    `json:"updates_rescan"`
	UpdatesRetryMS       int64  `json:"updates_retry_ms"`
	ResultTimeoutMS      int64  `json:"result_timeout_ms"`
	ResultRetryMS        int64  `json:"result_retry_ms"`
	ResultWait           int    `json:"result_wait"`
}

// Redact summarises the configuration without any secret value.
func (c Config) Redact() Redacted {
	return Redacted{
		Listen:               c.Listen(),
		Upstream:             c.Upstream,
		ReasoningEffort:      c.ReasoningEffort,
		RoutingEnabled:       c.RoutingEnabled(),
		UpdatesEnabled:       c.UpdatesEnabled(),
		QueueEnabled:         c.QueueEnabled(),
		QueueConsumerEnabled: c.QueueEnabledAndRouted(),
		ProfileEntity:        c.ProfileEntity,
		AnnounceScript:       c.AnnounceScript,
		Satellite:            c.Satellite,
		MediaPlayer:          c.MediaPlayer,
		AnnounceMaxChars:     c.AnnounceMaxChars,
		QueueConsumer:        c.QueueConsumer,
		SchedulerURL:         c.SchedulerURL,
		HAConfigured:         c.HAURL != "" && c.HAToken != "",
		GatewayConfigured:    c.GatewayURL != "" && c.GatewayToken != "",
		SchedulerAuth:        c.SchedulerURL != "" && c.SchedulerToken != "",
		AnnounceTTL:          c.AnnounceTTL,
		UpdatesRescan:        c.UpdatesRescan,
		UpdatesRetryMS:       c.UpdatesRetry.Milliseconds(),
		ResultTimeoutMS:      c.ResultTimeout.Milliseconds(),
		ResultRetryMS:        c.ResultRetry.Milliseconds(),
		ResultWait:           c.ResultWait,
	}
}

// readSecret reads a Bearer token from a mounted file, falling back to the
// inline environment value. A configured but unreadable file is empty and does
// not fall back, exactly like _read_secret() in voice_proxy.py. A nil accessor
// (a zero Config in a test) falls back to os.ReadFile.
func readSecret(path, inline string, readFile ReadFile) string {
	if path != "" {
		if readFile == nil {
			readFile = os.ReadFile
		}
		data, err := readFile(path)
		if err != nil {
			return ""
		}
		return strings.TrimSpace(string(data))
	}
	return strings.TrimSpace(inline)
}

// baseURL mirrors `os.environ.get(name, fallback).rstrip("/")`.
func baseURL(lookup LookupEnv, name, fallback string) string {
	value := stringEnv(lookup, name, fallback)
	return strings.TrimRight(value, "/")
}

// stringEnv mirrors `os.environ.get(name, fallback)`: the fallback applies only
// when the variable is absent.
func stringEnv(lookup LookupEnv, name, fallback string) string {
	if value, ok := lookup(name); ok {
		return value
	}
	return fallback
}

// switchedOff mirrors Python's `.strip().lower() in ("0", "false", "no", "off")`.
func switchedOff(value string) bool {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "0", "false", "no", "off":
		return true
	}
	return false
}

func intEnv(lookup LookupEnv, name string, fallback int) (int, error) {
	raw := stringEnv(lookup, name, strconv.Itoa(fallback))
	// Python int() accepts surrounding whitespace.
	value, err := strconv.Atoi(strings.TrimSpace(raw))
	if err != nil {
		return 0, fmt.Errorf("%s: %q is not an integer", name, raw)
	}
	return value, nil
}

func secondsEnv(lookup LookupEnv, name string, fallback float64) (time.Duration, error) {
	raw := stringEnv(lookup, name, strconv.FormatFloat(fallback, 'f', -1, 64))
	value, err := strconv.ParseFloat(strings.TrimSpace(raw), 64)
	if err != nil {
		return 0, fmt.Errorf("%s: %q is not a number", name, raw)
	}
	return time.Duration(value * float64(time.Second)), nil
}

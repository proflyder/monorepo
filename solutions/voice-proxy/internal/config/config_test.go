package config_test

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/config"
)

// env builds a LookupEnv from a map: absent keys behave like an unset variable.
func env(values map[string]string) config.LookupEnv {
	return func(name string) (string, bool) {
		value, ok := values[name]
		return value, ok
	}
}

func noFiles(string) ([]byte, error) { return nil, errors.New("not found") }

func load(t *testing.T, values map[string]string) config.Config {
	t.Helper()
	cfg, err := config.LoadFrom(env(values), noFiles)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	return cfg
}

// TestDefaults pins every documented default.
func TestDefaults(t *testing.T) {
	cfg := load(t, nil)
	checks := []struct {
		name string
		got  any
		want any
	}{
		{"upstream", cfg.Upstream, "http://127.0.0.1:8642"},
		{"bind", cfg.Bind, "127.0.0.1"},
		{"port", cfg.Port, 8643},
		{"reasoning_effort", cfg.ReasoningEffort, "low"},
		{"profile_entity", cfg.ProfileEntity, "input_select.ricardo_voice_profile"},
		{"profile_cache", cfg.ProfileCache, 2 * time.Second},
		{"ha_timeout", cfg.HATimeout, 3 * time.Second},
		{"gateway_timeout", cfg.GatewayTimeout, 5 * time.Second},
		{"result_timeout", cfg.ResultTimeout, 900 * time.Second},
		{"result_retry", cfg.ResultRetry, 2 * time.Second},
		{"result_wait", cfg.ResultWait, 25},
		{"updates_retry", cfg.UpdatesRetry, 2 * time.Second},
		{"announce_max_chars", cfg.AnnounceMaxChars, 200},
		{"announce_script", cfg.AnnounceScript, "script.schedule_announce"},
		{"satellite", cfg.Satellite, "assist_satellite.iandeks_stantsiia_midi_assist_satellite"},
		{"media_player", cfg.MediaPlayer, "media_player.iandeks_stantsiia_midi_media_player"},
		{"queue_consumer", cfg.QueueConsumer, "voice-proxy-home"},
		{"announce_ttl", cfg.AnnounceTTL, 10800},
		{"updates_rescan", cfg.UpdatesRescan, 300},
		{"updates_disabled", cfg.UpdatesDisabled, false},
		{"queue_disabled", cfg.QueueDisabled, false},
	}
	for _, check := range checks {
		if check.got != check.want {
			t.Errorf("%s = %v, want %v", check.name, check.got, check.want)
		}
	}
	if got, want := cfg.Listen(), "127.0.0.1:8643"; got != want {
		t.Errorf("Listen() = %q, want %q", got, want)
	}
}

// TestBaseURLsDropTrailingSlashes mirrors the module-level .rstrip("/").
func TestBaseURLsDropTrailingSlashes(t *testing.T) {
	cfg := load(t, map[string]string{
		"HERMES_UPSTREAM":          "http://hermes.example:8642///",
		"HERMES_HA_URL":            "http://ha.example/",
		"HERMES_VOICE_GATEWAY_URL": "http://gateway.example",
		"HERMES_SCHEDULER_URL":     "http://scheduler.example//",
	})
	if cfg.Upstream != "http://hermes.example:8642" {
		t.Errorf("upstream = %q", cfg.Upstream)
	}
	if cfg.HAURL != "http://ha.example" {
		t.Errorf("ha = %q", cfg.HAURL)
	}
	if cfg.GatewayURL != "http://gateway.example" {
		t.Errorf("gateway = %q", cfg.GatewayURL)
	}
	if cfg.SchedulerURL != "http://scheduler.example" {
		t.Errorf("scheduler = %q", cfg.SchedulerURL)
	}
}

// TestEmptyValuesAreNotDefaults pins os.environ.get(name, default): a variable
// that is set but empty is empty, not the default.
func TestEmptyValuesAreNotDefaults(t *testing.T) {
	cfg := load(t, map[string]string{"HERMES_VOICE_PROFILE_ENTITY": ""})
	if cfg.ProfileEntity != "" {
		t.Fatalf("profile entity = %q, want empty", cfg.ProfileEntity)
	}
	// Numeric variables are parsed, so an empty value is an error.
	if _, err := config.LoadFrom(env(map[string]string{"HERMES_PROXY_PORT": ""}), noFiles); err == nil {
		t.Fatal("an empty HERMES_PROXY_PORT should be rejected")
	}
}

// TestSecretsPreferFiles mirrors _read_secret: a *_FILE wins, an unreadable
// file yields an empty secret, and there is no fallback to the inline value.
func TestSecretsPreferFiles(t *testing.T) {
	files := map[string]string{
		"/run/secrets/ha":        "  file-ha-token\n",
		"/run/secrets/gateway":   "file-gateway-token",
		"/run/secrets/scheduler": "file-scheduler-token",
	}
	readFile := func(path string) ([]byte, error) {
		value, ok := files[path]
		if !ok {
			return nil, errors.New("not found")
		}
		return []byte(value), nil
	}
	cfg, err := config.LoadFrom(env(map[string]string{
		"HERMES_HA_TOKEN_FILE":            "/run/secrets/ha",
		"HERMES_HA_TOKEN":                 "inline-ha-token",
		"HERMES_VOICE_GATEWAY_TOKEN_FILE": "/run/secrets/gateway",
		"HERMES_VOICE_GATEWAY_TOKEN":      "inline-gateway-token",
		"HERMES_SCHEDULER_TOKEN_FILE":     "/run/secrets/missing",
		"HERMES_SCHEDULER_TOKEN":          "inline-scheduler-token",
	}), readFile)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if cfg.HAToken != "file-ha-token" {
		t.Errorf("ha token = %q, want the trimmed file value", cfg.HAToken)
	}
	if cfg.GatewayToken != "file-gateway-token" {
		t.Errorf("gateway token = %q", cfg.GatewayToken)
	}
	// An unreadable file is empty and does not fall back to the inline value.
	if cfg.SchedulerToken != "" {
		t.Errorf("scheduler token = %q, want empty", cfg.SchedulerToken)
	}
}

// TestSwitches pins the routing/updates/queue switches.
func TestSwitches(t *testing.T) {
	base := map[string]string{
		"HERMES_HA_URL":              "http://ha.example",
		"HERMES_HA_TOKEN":            "ha",
		"HERMES_VOICE_GATEWAY_URL":   "http://gateway.example",
		"HERMES_VOICE_GATEWAY_TOKEN": "gateway",
		"HERMES_SCHEDULER_URL":       "http://scheduler.example",
		"HERMES_SCHEDULER_TOKEN":     "scheduler",
	}
	cfg := load(t, base)
	if !cfg.RoutingEnabled() || !cfg.UpdatesEnabled() || !cfg.QueueEnabled() {
		t.Fatalf("fully configured: routing=%v updates=%v queue=%v",
			cfg.RoutingEnabled(), cfg.UpdatesEnabled(), cfg.QueueEnabled())
	}
	if !cfg.QueueEnabledAndRouted() {
		t.Error("QueueEnabledAndRouted() should be true")
	}
	if got := cfg.QueueName("work"); got != "voice-work" {
		t.Errorf("QueueName = %q", got)
	}

	// A partially configured path keeps the previous behaviour.
	for _, missing := range []string{
		"HERMES_HA_URL", "HERMES_HA_TOKEN",
		"HERMES_VOICE_GATEWAY_URL", "HERMES_VOICE_GATEWAY_TOKEN",
	} {
		partial := map[string]string{}
		for key, value := range base {
			partial[key] = value
		}
		delete(partial, missing)
		if load(t, partial).RoutingEnabled() {
			t.Errorf("routing enabled without %s", missing)
		}
	}
	for _, missing := range []string{"HERMES_SCHEDULER_URL", "HERMES_SCHEDULER_TOKEN"} {
		partial := map[string]string{}
		for key, value := range base {
			partial[key] = value
		}
		delete(partial, missing)
		if load(t, partial).QueueEnabled() {
			t.Errorf("queue enabled without %s", missing)
		}
	}

	// "0", "false", "no" and "off" switch the workers off; routing still gates
	// updates.
	for _, value := range []string{"0", "false", "no", "off", " OFF "} {
		off := load(t, with(base, "HERMES_VOICE_QUEUE", value, "HERMES_VOICE_UPDATES", value))
		if off.QueueEnabled() || off.UpdatesEnabled() {
			t.Errorf("%q did not switch the workers off: queue=%v updates=%v",
				value, off.QueueEnabled(), off.UpdatesEnabled())
		}
	}
	on := load(t, with(base, "HERMES_VOICE_QUEUE", "1", "HERMES_VOICE_UPDATES", "1"))
	if !on.QueueEnabled() || !on.UpdatesEnabled() {
		t.Error("the default switch value must stay on")
	}
	// The consumer needs routing too.
	noRouting := map[string]string{
		"HERMES_SCHEDULER_URL":   "http://scheduler.example",
		"HERMES_SCHEDULER_TOKEN": "scheduler",
	}
	if load(t, noRouting).QueueEnabledAndRouted() {
		t.Error("the queue consumer must not start without routing")
	}
}

// TestUpdatesSwitchIsDefaultOnAndZeroDisables mirrors
// test_updates_switch_is_default_on_and_zero_disables.
func TestUpdatesSwitchIsDefaultOnAndZeroDisables(t *testing.T) {
	routed := map[string]string{
		"HERMES_HA_URL":              "http://ha.example",
		"HERMES_HA_TOKEN":            "ha",
		"HERMES_VOICE_GATEWAY_URL":   "http://gateway.example",
		"HERMES_VOICE_GATEWAY_TOKEN": "gateway",
	}
	if !load(t, routed).UpdatesEnabled() {
		t.Fatal("updates must be on by default")
	}
	if load(t, with(routed, "HERMES_VOICE_UPDATES", "0")).UpdatesEnabled() {
		t.Fatal("HERMES_VOICE_UPDATES=0 must disable updates")
	}
	unrouted := map[string]string{"HERMES_VOICE_UPDATES": "1"}
	if load(t, unrouted).UpdatesEnabled() {
		t.Fatal("updates must not run without routing")
	}
}

// TestQueueSwitchNeedsURLAndToken mirrors test_queue_switch_needs_url_and_token.
func TestQueueSwitchNeedsURLAndToken(t *testing.T) {
	base := map[string]string{
		"HERMES_SCHEDULER_URL":   "http://scheduler.example",
		"HERMES_SCHEDULER_TOKEN": "scheduler",
	}
	if !load(t, base).QueueEnabled() {
		t.Fatal("a fully configured queue must be enabled")
	}
	if load(t, map[string]string{"HERMES_SCHEDULER_TOKEN": "scheduler"}).QueueEnabled() {
		t.Fatal("the queue must need a URL")
	}
	if load(t, map[string]string{"HERMES_SCHEDULER_URL": "http://scheduler.example"}).QueueEnabled() {
		t.Fatal("the queue must need a token")
	}
	if load(t, with(base, "HERMES_VOICE_QUEUE", "0")).QueueEnabled() {
		t.Fatal("HERMES_VOICE_QUEUE=0 must disable the queue")
	}
}

// TestSecretFilesAreReadOnEveryUse pins the lazy *_FILE semantics: a
// ha_token()/voice_token()/scheduler_token() re-read the mounted file, so a
// rotated token needs no restart.
func TestSecretFilesAreReadOnEveryUse(t *testing.T) {
	token := "first-token"
	readFile := func(string) ([]byte, error) { return []byte(token + "\n"), nil }
	cfg, err := config.LoadFrom(env(map[string]string{
		"HERMES_HA_TOKEN_FILE": "/run/secrets/ha",
		"HERMES_HA_TOKEN":      "inline",
	}), readFile)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if got := cfg.HAToken; got != "first-token" {
		t.Fatalf("token = %q", got)
	}
	token = "rotated-token"
	if got := cfg.HATokenValue(); got != "rotated-token" {
		t.Fatalf("token after rotation = %q", got)
	}
	// The inline fallback is only used without a *_FILE.
	inline := load(t, map[string]string{"HERMES_HA_TOKEN": "inline-token"})
	if got := inline.HATokenValue(); got != "inline-token" {
		t.Fatalf("inline token = %q", got)
	}
}

func TestNumericParsing(t *testing.T) {
	cfg := load(t, map[string]string{
		"HERMES_PROXY_PORT":               "9000",
		"HERMES_VOICE_PROFILE_CACHE_MS":   "0",
		"HERMES_HA_TIMEOUT":               "1.5",
		"HERMES_VOICE_ANNOUNCE_MAX_CHARS": "50",
	})
	if cfg.Port != 9000 || cfg.ProfileCache != 0 || cfg.HATimeout != 1500*time.Millisecond ||
		cfg.AnnounceMaxChars != 50 {
		t.Fatalf("parsed = %#v", cfg)
	}
	if _, err := config.LoadFrom(env(map[string]string{"HERMES_HA_TIMEOUT": "soon"}), noFiles); err == nil {
		t.Fatal("a non-numeric timeout should be rejected")
	}
	if _, err := config.LoadFrom(env(map[string]string{"HERMES_VOICE_ANNOUNCE_TTL": "1.5"}), noFiles); err == nil {
		t.Fatal("a fractional integer should be rejected")
	}
}

// TestRedactHasNoSecretValues pins the -status contract at the config level.
func TestRedactHasNoSecretValues(t *testing.T) {
	cfg := load(t, map[string]string{
		"HERMES_HA_URL":              "http://ha.example",
		"HERMES_HA_TOKEN":            "ha-secret",
		"HERMES_VOICE_GATEWAY_URL":   "http://gateway.example",
		"HERMES_VOICE_GATEWAY_TOKEN": "gateway-secret",
		"HERMES_SCHEDULER_URL":       "http://scheduler.example",
		"HERMES_SCHEDULER_TOKEN":     "scheduler-secret",
	})
	encoded, err := json.Marshal(cfg.Redact())
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	text := string(encoded)
	for _, secret := range []string{"ha-secret", "gateway-secret", "scheduler-secret"} {
		if strings.Contains(text, secret) {
			t.Fatalf("redacted config leaked %q: %s", secret, text)
		}
	}
}

func with(base map[string]string, pairs ...string) map[string]string {
	out := map[string]string{}
	for key, value := range base {
		out[key] = value
	}
	for i := 0; i+1 < len(pairs); i += 2 {
		out[pairs[i]] = pairs[i+1]
	}
	return out
}

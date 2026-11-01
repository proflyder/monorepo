// Package config loads mcp-gateway settings from the environment only.
//
// Secrets are never read from environment values: every secret has a *_FILE
// variable, so the process can run with a read-only projected volume and the
// deployment never embeds a token in a manifest.
package config

import (
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

// Defaults documented in README.md.
const (
	DefaultHTTPAddr         = ":8080"
	DefaultGroups           = "infra.read"
	DefaultPendingTTL       = 90 * time.Second
	DefaultCooldown         = 5 * time.Minute
	DefaultSelfRestartDelay = 20 * time.Second
	DefaultQueryTimeout     = 4 * time.Second
	DefaultStatusTimeout    = 8 * time.Second
	DefaultReportTimeout    = 9 * time.Second
	DefaultLogByteCap       = 64 << 10
	DefaultTelegramAPIURL   = "https://api.telegram.org"
)

// Config is a validated process configuration.
type Config struct {
	HTTPAddr  string
	AuthToken string
	Groups    []string

	CatalogFile string

	PrometheusURL   string
	AlertmanagerURL string
	LokiURL         string

	// Home Assistant REST endpoint for ha_* catalog checks. Empty disables
	// them; they then report "not configured" instead of failing.
	HAURL       string
	HATokenFile string
	HAToken     string

	KubeAPIURL    string
	KubeTokenFile string
	KubeCAFile    string

	TelegramAPIURL    string
	TelegramTokenFile string
	TelegramChatFile  string
	TelegramToken     string
	TelegramChatID    string

	PendingTTL       time.Duration
	Cooldown         time.Duration
	SelfRestartDelay time.Duration
	QueryTimeout     time.Duration
	StatusTimeout    time.Duration
	ReportTimeout    time.Duration
	LogByteCap       int
}

// Load reads and validates the process environment.
func Load() (Config, error) { return load(os.Getenv, os.ReadFile) }

func load(getenv func(string) string, readFile func(string) ([]byte, error)) (Config, error) {
	cfg := Config{
		HTTPAddr:          valueOr(getenv, "MCP_HTTP_ADDR", DefaultHTTPAddr),
		CatalogFile:       strings.TrimSpace(getenv("MCP_INFRA_CATALOG_FILE")),
		PrometheusURL:     strings.TrimSpace(getenv("MCP_PROMETHEUS_URL")),
		AlertmanagerURL:   strings.TrimSpace(getenv("MCP_ALERTMANAGER_URL")),
		LokiURL:           strings.TrimSpace(getenv("MCP_LOKI_URL")),
		HAURL:             strings.TrimSpace(getenv("MCP_HA_URL")),
		HATokenFile:       strings.TrimSpace(getenv("MCP_HA_TOKEN_FILE")),
		KubeAPIURL:        strings.TrimSpace(getenv("KUBERNETES_API_URL")),
		KubeTokenFile:     strings.TrimSpace(getenv("KUBERNETES_TOKEN_FILE")),
		KubeCAFile:        strings.TrimSpace(getenv("KUBERNETES_CA_FILE")),
		TelegramAPIURL:    valueOr(getenv, "TELEGRAM_API_URL", DefaultTelegramAPIURL),
		TelegramTokenFile: strings.TrimSpace(getenv("TELEGRAM_BOT_TOKEN_FILE")),
		TelegramChatFile:  strings.TrimSpace(getenv("TELEGRAM_CHAT_ID_FILE")),
	}

	cfg.Groups = splitList(valueOr(getenv, "MCP_GROUPS", DefaultGroups))
	if len(cfg.Groups) == 0 {
		return cfg, fmt.Errorf("MCP_GROUPS must contain at least one group")
	}

	tokenFile := strings.TrimSpace(getenv("MCP_AUTH_TOKEN_FILE"))
	if tokenFile == "" {
		return cfg, fmt.Errorf("MCP_AUTH_TOKEN_FILE is required")
	}
	token, err := readSecret(readFile, tokenFile, "MCP_AUTH_TOKEN_FILE")
	if err != nil {
		return cfg, err
	}
	cfg.AuthToken = token

	if cfg.PendingTTL, err = durationValue(getenv, "MCP_PENDING_TTL", DefaultPendingTTL, time.Second, time.Hour); err != nil {
		return cfg, err
	}
	if cfg.Cooldown, err = durationValue(getenv, "MCP_RESTART_COOLDOWN", DefaultCooldown, time.Second, 24*time.Hour); err != nil {
		return cfg, err
	}
	if cfg.SelfRestartDelay, err = durationValue(getenv, "MCP_SELF_RESTART_DELAY", DefaultSelfRestartDelay, 0, 10*time.Minute); err != nil {
		return cfg, err
	}
	if cfg.QueryTimeout, err = durationValue(getenv, "MCP_QUERY_TIMEOUT", DefaultQueryTimeout, time.Second, 30*time.Second); err != nil {
		return cfg, err
	}
	if cfg.StatusTimeout, err = durationValue(getenv, "MCP_STATUS_TIMEOUT", DefaultStatusTimeout, time.Second, 60*time.Second); err != nil {
		return cfg, err
	}
	if cfg.ReportTimeout, err = durationValue(getenv, "MCP_REPORT_TIMEOUT", DefaultReportTimeout, time.Second, 60*time.Second); err != nil {
		return cfg, err
	}
	if cfg.LogByteCap, err = intValue(getenv, "MCP_LOG_BYTE_CAP", DefaultLogByteCap, 1024, 4<<20); err != nil {
		return cfg, err
	}

	if err := validateGroups(cfg); err != nil {
		return cfg, err
	}
	if err := loadTelegramFiles(&cfg, getenv, readFile); err != nil {
		return cfg, err
	}
	if err := loadHAFiles(&cfg, readFile); err != nil {
		return cfg, err
	}
	return cfg, nil
}

func loadHAFiles(cfg *Config, readFile func(string) ([]byte, error)) error {
	if cfg.HAURL == "" && cfg.HATokenFile == "" {
		return nil
	}
	if cfg.HAURL == "" || cfg.HATokenFile == "" {
		return fmt.Errorf("MCP_HA_URL and MCP_HA_TOKEN_FILE must be set together")
	}
	token, err := readSecret(readFile, cfg.HATokenFile, "MCP_HA_TOKEN_FILE")
	if err != nil {
		return err
	}
	cfg.HAToken = token
	return nil
}

func validateGroups(cfg Config) error {
	needCatalog := false
	needReport := false
	for _, name := range cfg.Groups {
		switch name {
		case "infra.read", "infra.fix":
			needCatalog = true
		case "infra.report":
			needReport = true
			if cfg.TelegramTokenFile == "" || cfg.TelegramChatFile == "" {
				return fmt.Errorf("group infra.report requires TELEGRAM_BOT_TOKEN_FILE and TELEGRAM_CHAT_ID_FILE")
			}
		default:
			return fmt.Errorf("unknown tool group %q", name)
		}
	}
	if needCatalog && cfg.CatalogFile == "" {
		return fmt.Errorf("MCP_INFRA_CATALOG_FILE is required for group infra.read/infra.fix")
	}
	if needReport && cfg.TelegramTokenFile == "" {
		return fmt.Errorf("TELEGRAM_BOT_TOKEN_FILE is required for group infra.report")
	}
	return nil
}

func loadTelegramFiles(cfg *Config, getenv func(string) string, readFile func(string) ([]byte, error)) error {
	enabled := false
	for _, name := range cfg.Groups {
		if name == "infra.report" {
			enabled = true
		}
	}
	if !enabled {
		return nil
	}
	token, err := readSecret(readFile, cfg.TelegramTokenFile, "TELEGRAM_BOT_TOKEN_FILE")
	if err != nil {
		return err
	}
	chat, err := readSecret(readFile, cfg.TelegramChatFile, "TELEGRAM_CHAT_ID_FILE")
	if err != nil {
		return err
	}
	// Some deployments store a comma-separated list; the gateway always uses
	// the first entry so the model can never select a recipient.
	if idx := strings.IndexByte(chat, ','); idx >= 0 {
		chat = strings.TrimSpace(chat[:idx])
	}
	if chat == "" {
		return fmt.Errorf("TELEGRAM_CHAT_ID_FILE is empty")
	}
	cfg.TelegramToken = token
	cfg.TelegramChatID = chat
	return nil
}

func readSecret(readFile func(string) ([]byte, error), path, variable string) (string, error) {
	data, err := readFile(path)
	if err != nil {
		return "", fmt.Errorf("%s: %w", variable, err)
	}
	value := strings.TrimSpace(string(data))
	if value == "" {
		return "", fmt.Errorf("%s is empty", variable)
	}
	return value, nil
}

func valueOr(getenv func(string) string, key, def string) string {
	if v := strings.TrimSpace(getenv(key)); v != "" {
		return v
	}
	return def
}

func splitList(raw string) []string {
	var out []string
	for _, part := range strings.Split(raw, ",") {
		if v := strings.TrimSpace(part); v != "" {
			out = append(out, v)
		}
	}
	return out
}

func durationValue(getenv func(string) string, key string, def, min, max time.Duration) (time.Duration, error) {
	raw := strings.TrimSpace(getenv(key))
	if raw == "" {
		return def, nil
	}
	value, err := time.ParseDuration(raw)
	if err != nil || value < min || value > max {
		return def, fmt.Errorf("%s must be a duration in %s..%s", key, min, max)
	}
	return value, nil
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

// HealthcheckURL turns the listen address into a loopback base URL for the
// -healthcheck probe.
func HealthcheckURL(listen string) (string, error) {
	host, port, err := net.SplitHostPort(listen)
	if err != nil {
		return "", fmt.Errorf("invalid MCP_HTTP_ADDR: %w", err)
	}
	if host == "" || host == "0.0.0.0" || host == "::" {
		host = "127.0.0.1"
	}
	return "http://" + net.JoinHostPort(host, port), nil
}

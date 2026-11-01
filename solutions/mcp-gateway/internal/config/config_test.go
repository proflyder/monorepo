package config

import (
	"errors"
	"strings"
	"testing"
	"time"
	_ "time/tzdata" // keep IANA zone assertions independent of host zoneinfo
)

func loader(env map[string]string, files map[string]string) (Config, error) {
	return load(func(key string) string { return env[key] }, func(path string) ([]byte, error) {
		value, ok := files[path]
		if !ok {
			return nil, errors.New("no such file")
		}
		return []byte(value), nil
	})
}

func baseEnv() map[string]string {
	return map[string]string{
		"MCP_AUTH_TOKEN_FILE":    "/run/secrets/mcp-token",
		"MCP_INFRA_CATALOG_FILE": "/etc/mcp-gateway/catalog.yaml",
	}
}

func baseFiles() map[string]string {
	return map[string]string{"/run/secrets/mcp-token": "s3cret-token\n"}
}

func TestLoadDefaults(t *testing.T) {
	cfg, err := loader(baseEnv(), baseFiles())
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if cfg.AuthToken != "s3cret-token" {
		t.Fatalf("token = %q", cfg.AuthToken)
	}
	if cfg.HTTPAddr != DefaultHTTPAddr {
		t.Fatalf("addr = %q", cfg.HTTPAddr)
	}
	if len(cfg.Groups) != 1 || cfg.Groups[0] != "infra.read" {
		t.Fatalf("groups = %v", cfg.Groups)
	}
	if cfg.PendingTTL != DefaultPendingTTL || cfg.Cooldown != DefaultCooldown {
		t.Fatalf("ttl=%v cooldown=%v", cfg.PendingTTL, cfg.Cooldown)
	}
	if cfg.SelfRestartDelay != DefaultSelfRestartDelay || cfg.QueryTimeout != DefaultQueryTimeout {
		t.Fatalf("delay=%v query=%v", cfg.SelfRestartDelay, cfg.QueryTimeout)
	}
	if cfg.LogByteCap != DefaultLogByteCap || cfg.StatusTimeout != DefaultStatusTimeout {
		t.Fatalf("cap=%d status=%v", cfg.LogByteCap, cfg.StatusTimeout)
	}
	if cfg.ReportTimeout != DefaultReportTimeout {
		t.Fatalf("report timeout = %v", cfg.ReportTimeout)
	}
	if cfg.Timezone != DefaultTimezone || cfg.Location == nil || cfg.Location.String() != "UTC" {
		t.Fatalf("timezone = %q, location = %v", cfg.Timezone, cfg.Location)
	}
	if cfg.HAURL != "" || cfg.HAToken != "" {
		t.Fatalf("HA unexpectedly configured: %q %q", cfg.HAURL, cfg.HAToken)
	}
}

func TestLoadOverrides(t *testing.T) {
	env := baseEnv()
	env["MCP_GROUPS"] = "infra.read, infra.fix, infra.report"
	env["TELEGRAM_BOT_TOKEN_FILE"] = "/run/secrets/tg-token"
	env["TELEGRAM_CHAT_ID_FILE"] = "/run/secrets/tg-chat"
	env["MCP_PENDING_TTL"] = "45s"
	env["MCP_RESTART_COOLDOWN"] = "10m"
	env["MCP_SELF_RESTART_DELAY"] = "5s"
	env["MCP_QUERY_TIMEOUT"] = "2s"
	env["MCP_STATUS_TIMEOUT"] = "6s"
	env["MCP_LOG_BYTE_CAP"] = "2048"
	env["MCP_REPORT_TIMEOUT"] = "7s"
	env["MCP_HA_URL"] = "http://192.0.2.28:8123"
	env["MCP_HA_TOKEN_FILE"] = "/run/secrets/ha-token"
	env["MCP_HTTP_ADDR"] = ":9000"
	env["MCP_TIMEZONE"] = "Asia/Almaty"
	files := baseFiles()
	files["/run/secrets/tg-token"] = "123:ABC"
	files["/run/secrets/tg-chat"] = "-100123,-100456\n"
	files["/run/secrets/ha-token"] = "ha-long-lived\n"
	cfg, err := loader(env, files)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if cfg.PendingTTL != 45*time.Second || cfg.Cooldown != 10*time.Minute || cfg.SelfRestartDelay != 5*time.Second {
		t.Fatalf("durations = %v %v %v", cfg.PendingTTL, cfg.Cooldown, cfg.SelfRestartDelay)
	}
	if cfg.QueryTimeout != 2*time.Second || cfg.StatusTimeout != 6*time.Second || cfg.LogByteCap != 2048 {
		t.Fatalf("timeouts = %v %v %d", cfg.QueryTimeout, cfg.StatusTimeout, cfg.LogByteCap)
	}
	if cfg.ReportTimeout != 7*time.Second {
		t.Fatalf("report timeout = %v", cfg.ReportTimeout)
	}
	if cfg.HAURL != "http://192.0.2.28:8123" || cfg.HAToken != "ha-long-lived" {
		t.Fatalf("HA = %q %q", cfg.HAURL, cfg.HAToken)
	}
	if cfg.TelegramToken != "123:ABC" || cfg.TelegramChatID != "-100123" {
		t.Fatalf("telegram = %q %q", cfg.TelegramToken, cfg.TelegramChatID)
	}
	if cfg.HTTPAddr != ":9000" {
		t.Fatalf("addr = %q", cfg.HTTPAddr)
	}
	if cfg.Timezone != "Asia/Almaty" || cfg.Location.String() != "Asia/Almaty" {
		t.Fatalf("timezone = %q, location = %v", cfg.Timezone, cfg.Location)
	}
}

func TestLoadErrors(t *testing.T) {
	tests := map[string]struct {
		env   map[string]string
		files map[string]string
		want  string
	}{
		"missing auth file": {
			env:   map[string]string{"MCP_INFRA_CATALOG_FILE": "/c"},
			files: baseFiles(),
			want:  "MCP_AUTH_TOKEN_FILE",
		},
		"empty auth file": {
			env:   baseEnv(),
			files: map[string]string{"/run/secrets/mcp-token": "   \n"},
			want:  "empty",
		},
		"unknown group": {
			env:   withGroup("infra.unknown"),
			files: baseFiles(),
			want:  "unknown tool group",
		},
		"missing catalog": {
			env:   map[string]string{"MCP_AUTH_TOKEN_FILE": "/run/secrets/mcp-token"},
			files: baseFiles(),
			want:  "MCP_INFRA_CATALOG_FILE",
		},
		"report without telegram": {
			env:   withGroup("infra.report"),
			files: baseFiles(),
			want:  "TELEGRAM_BOT_TOKEN_FILE",
		},
		"bad ttl": {
			env:   withEnv(baseEnv(), "MCP_PENDING_TTL", "forever"),
			files: baseFiles(),
			want:  "MCP_PENDING_TTL",
		},
		"ttl too small": {
			env:   withEnv(baseEnv(), "MCP_PENDING_TTL", "10ms"),
			files: baseFiles(),
			want:  "MCP_PENDING_TTL",
		},
		"bad byte cap": {
			env:   withEnv(baseEnv(), "MCP_LOG_BYTE_CAP", "10"),
			files: baseFiles(),
			want:  "MCP_LOG_BYTE_CAP",
		},
		"HA url without token": {
			env:   withEnv(baseEnv(), "MCP_HA_URL", "http://192.0.2.28:8123"),
			files: baseFiles(),
			want:  "MCP_HA_TOKEN_FILE",
		},
		"HA token file missing": {
			env:   withEnv(withEnv(baseEnv(), "MCP_HA_URL", "http://192.0.2.28:8123"), "MCP_HA_TOKEN_FILE", "/nope"),
			files: baseFiles(),
			want:  "MCP_HA_TOKEN_FILE",
		},
		"unknown timezone": {
			env:   withEnv(baseEnv(), "MCP_TIMEZONE", "Nowhere/Nope"),
			files: baseFiles(),
			want:  "MCP_TIMEZONE",
		},
	}
	for name, tc := range tests {
		t.Run(name, func(t *testing.T) {
			_, err := loader(tc.env, tc.files)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v, want %q", err, tc.want)
			}
		})
	}
}

func withGroup(group string) map[string]string {
	return withEnv(baseEnv(), "MCP_GROUPS", group)
}

func withEnv(env map[string]string, key, value string) map[string]string {
	out := map[string]string{}
	for k, v := range env {
		out[k] = v
	}
	out[key] = value
	return out
}

func TestHealthcheckURL(t *testing.T) {
	cases := map[string]string{
		":8080":        "http://127.0.0.1:8080",
		"0.0.0.0:8080": "http://127.0.0.1:8080",
		"127.0.0.1:9":  "http://127.0.0.1:9",
		"[::1]:8080":   "http://[::1]:8080",
	}
	for input, want := range cases {
		got, err := HealthcheckURL(input)
		if err != nil {
			t.Fatalf("HealthcheckURL(%q): %v", input, err)
		}
		if got != want {
			t.Fatalf("HealthcheckURL(%q) = %q, want %q", input, got, want)
		}
	}
	if _, err := HealthcheckURL("nonsense"); err == nil {
		t.Fatal("invalid address accepted")
	}
}

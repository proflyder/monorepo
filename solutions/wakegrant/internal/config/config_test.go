package config

import (
	"strings"
	"testing"
	"time"
)

func reader(files map[string]string) func(string) ([]byte, error) {
	return func(path string) ([]byte, error) {
		value, ok := files[path]
		if !ok {
			return nil, errNotFound
		}
		return []byte(value), nil
	}
}

type notFoundError struct{}

func (notFoundError) Error() string { return "not found" }

var errNotFound = notFoundError{}

func getenv(values map[string]string) func(string) string {
	return func(key string) string { return values[key] }
}

func validEnv() map[string]string {
	return map[string]string{
		"WAKE_GRANT_ISSUER_TOKEN_FILE": "/secrets/issuer_token",
		"WAKE_GRANT_MACHINES_FILE":     "/secrets/machines",
	}
}

func TestLoadDefaults(t *testing.T) {
	cfg, err := load(getenv(validEnv()), reader(map[string]string{
		"/secrets/issuer_token": "token\n",
	}))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if cfg.Listen != DefaultListen {
		t.Errorf("Listen = %q", cfg.Listen)
	}
	if cfg.DBPath != DefaultDBPath {
		t.Errorf("DBPath = %q", cfg.DBPath)
	}
	if cfg.MaxTTL != MaxTTL {
		t.Errorf("MaxTTL = %s", cfg.MaxTTL)
	}
	if cfg.IssuerToken != "token" {
		t.Errorf("IssuerToken was not trimmed")
	}
}

func TestLoadMaxTTL(t *testing.T) {
	env := validEnv()
	env["WAKE_GRANT_MAX_TTL_SECONDS"] = "120"
	cfg, err := load(getenv(env), reader(map[string]string{"/secrets/issuer_token": "token"}))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if cfg.MaxTTL != 120*time.Second {
		t.Errorf("MaxTTL = %s, want 120s", cfg.MaxTTL)
	}
	for _, value := range []string{"29", "601", "abc"} {
		env["WAKE_GRANT_MAX_TTL_SECONDS"] = value
		if _, err := load(getenv(env), reader(map[string]string{"/secrets/issuer_token": "token"})); err == nil {
			t.Errorf("WAKE_GRANT_MAX_TTL_SECONDS=%s: expected an error", value)
		}
	}
}

func TestLoadRequiresFilesAndToken(t *testing.T) {
	for name, env := range map[string]map[string]string{
		"no token file":    {"WAKE_GRANT_MACHINES_FILE": "/secrets/machines"},
		"no machines file": {"WAKE_GRANT_ISSUER_TOKEN_FILE": "/secrets/issuer_token"},
	} {
		if _, err := load(getenv(env), reader(map[string]string{"/secrets/issuer_token": "token"})); err == nil {
			t.Errorf("%s: expected an error", name)
		}
	}
	if _, err := load(getenv(validEnv()), reader(map[string]string{})); err == nil {
		t.Error("unreadable token file: expected an error")
	}
	for _, token := range []string{"", "  \n"} {
		if _, err := load(getenv(validEnv()), reader(map[string]string{"/secrets/issuer_token": token})); err == nil {
			t.Errorf("empty token %q: expected an error", token)
		}
	}
}

func TestUnknownEnvironmentIsIgnored(t *testing.T) {
	env := validEnv()
	env["SOMETHING_ELSE"] = strings.Repeat("x", 10)
	if _, err := load(getenv(env), reader(map[string]string{"/secrets/issuer_token": "token"})); err != nil {
		t.Fatalf("load: %v", err)
	}
}

func TestHealthcheckURLUsesLoopback(t *testing.T) {
	cases := map[string]string{
		"0.0.0.0:8099":   "http://127.0.0.1:8099",
		":8099":          "http://127.0.0.1:8099",
		"127.0.0.1:9000": "http://127.0.0.1:9000",
	}
	for listen, want := range cases {
		got, err := HealthcheckURL(listen)
		if err != nil {
			t.Fatalf("HealthcheckURL(%q): %v", listen, err)
		}
		if got != want {
			t.Errorf("HealthcheckURL(%q) = %q, want %q", listen, got, want)
		}
	}
	if _, err := HealthcheckURL("nonsense"); err == nil {
		t.Error("HealthcheckURL without a port: expected an error")
	}
}

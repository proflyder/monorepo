// Package config loads wakegrant settings from the environment only.
//
// Secrets are never read from environment values: the issuer token and the
// per-machine keys have *_FILE variables, so the process runs with a
// read-only mounted Secret and the deployment never embeds a token.
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
	DefaultListen = "127.0.0.1:8099"
	DefaultDBPath = "wakegrant.db"
	// MinTTL and MaxTTL bound ttlSeconds exactly like PROTOCOL.md.
	MinTTL     = 30 * time.Second
	MaxTTL     = 600 * time.Second
	DefaultTTL = 180 * time.Second
)

// Config is a validated process configuration.
type Config struct {
	Listen      string
	DBPath      string
	IssuerToken string
	// IssuerTokenFile and MachinesFile are kept for the healthcheck and for
	// logging; the values themselves are already loaded.
	IssuerTokenFile string
	MachinesFile    string
	MaxTTL          time.Duration
}

// Load reads and validates the process environment.
func Load() (Config, error) { return load(os.Getenv, os.ReadFile) }

func load(getenv func(string) string, readFile func(string) ([]byte, error)) (Config, error) {
	cfg := Config{
		Listen:          strings.TrimSpace(getenv("WAKE_GRANT_LISTEN")),
		DBPath:          strings.TrimSpace(getenv("WAKE_GRANT_DB_PATH")),
		IssuerTokenFile: strings.TrimSpace(getenv("WAKE_GRANT_ISSUER_TOKEN_FILE")),
		MachinesFile:    strings.TrimSpace(getenv("WAKE_GRANT_MACHINES_FILE")),
		MaxTTL:          MaxTTL,
	}
	if cfg.Listen == "" {
		cfg.Listen = DefaultListen
	}
	if cfg.DBPath == "" {
		cfg.DBPath = DefaultDBPath
	}
	if cfg.IssuerTokenFile == "" {
		return cfg, fmt.Errorf("WAKE_GRANT_ISSUER_TOKEN_FILE is required")
	}
	if cfg.MachinesFile == "" {
		return cfg, fmt.Errorf("WAKE_GRANT_MACHINES_FILE is required")
	}

	token, err := readFile(cfg.IssuerTokenFile)
	if err != nil {
		return cfg, fmt.Errorf("WAKE_GRANT_ISSUER_TOKEN_FILE: cannot read token file")
	}
	cfg.IssuerToken = strings.TrimSpace(string(token))
	if cfg.IssuerToken == "" {
		return cfg, fmt.Errorf("WAKE_GRANT_ISSUER_TOKEN_FILE: token is empty")
	}

	if raw := strings.TrimSpace(getenv("WAKE_GRANT_MAX_TTL_SECONDS")); raw != "" {
		seconds, err := strconv.Atoi(raw)
		if err != nil {
			return cfg, fmt.Errorf("WAKE_GRANT_MAX_TTL_SECONDS: %q is not an integer", raw)
		}
		cfg.MaxTTL = time.Duration(seconds) * time.Second
	}
	if cfg.MaxTTL < MinTTL || cfg.MaxTTL > MaxTTL {
		return cfg, fmt.Errorf(
			"WAKE_GRANT_MAX_TTL_SECONDS: must be between %d and %d",
			int(MinTTL/time.Second), int(MaxTTL/time.Second),
		)
	}
	return cfg, nil
}

// HealthcheckURL turns the listen address into a loopback base URL, so the
// in-container probe never depends on the pod IP.
func HealthcheckURL(listen string) (string, error) {
	host, port, err := net.SplitHostPort(listen)
	if err != nil {
		return "", fmt.Errorf("invalid WAKE_GRANT_LISTEN %q: %w", listen, err)
	}
	switch host {
	case "", "0.0.0.0", "::", "[::]":
		host = "127.0.0.1"
	}
	return "http://" + net.JoinHostPort(host, port), nil
}

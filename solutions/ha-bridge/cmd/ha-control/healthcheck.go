// Healthcheck for the ha-control container.
//
// The catalog server has no unauthenticated endpoint, so readiness is an
// authenticated GET /v1/catalog against the local listener. The bearer token is
// read from CONTROL_TOKEN(_FILE) by the normal config loader and never appears
// in probe arguments or Kubernetes events.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

const healthcheckTimeout = 4 * time.Second

type catalogHealth struct {
	Entries []json.RawMessage `json:"entries"`
}

// runHealthcheck returns a process exit code.
func runHealthcheck() int {
	cfg, err := loadSettings(os.Getenv)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck: configuration:", err)
		return 1
	}
	addr, err := healthcheckAddress(cfg.Listen)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	if err := checkCatalog(ctx, "http://"+addr, cfg.ControlToken, http.DefaultClient); err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	return 0
}

// healthcheckAddress maps the listen address to loopback for the probe.
func healthcheckAddress(listen string) (string, error) {
	host, port, err := net.SplitHostPort(listen)
	if err != nil {
		return "", fmt.Errorf("invalid CONTROL_LISTEN: %w", err)
	}
	if host == "" || host == "0.0.0.0" || host == "::" {
		host = "127.0.0.1"
	}
	return net.JoinHostPort(host, port), nil
}

// checkCatalog calls GET /v1/catalog with the bearer token.
func checkCatalog(ctx context.Context, baseURL, token string, hc *http.Client) error {
	req, err := http.NewRequestWithContext(
		ctx, http.MethodGet, strings.TrimSuffix(baseURL, "/")+"/v1/catalog", nil,
	)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/json")

	resp, err := hc.Do(req)
	if err != nil {
		return fmt.Errorf("catalog request failed: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return fmt.Errorf("catalog response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("catalog returned HTTP %d", resp.StatusCode)
	}
	var parsed catalogHealth
	if err := json.Unmarshal(body, &parsed); err != nil {
		return fmt.Errorf("catalog response is not JSON: %w", err)
	}
	if parsed.Entries == nil {
		return fmt.Errorf("catalog response has no entries array")
	}
	return nil
}

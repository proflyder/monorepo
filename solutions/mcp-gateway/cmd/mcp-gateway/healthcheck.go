// Healthcheck for the distroless gateway container.
//
// The probe runs the binary as `mcp-gateway -healthcheck`: no shell, curl or
// wget exists in the image, and no secret ever appears on a command line.
package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/config"
)

const healthcheckTimeout = 4 * time.Second

// runHealthcheck returns a process exit code.
func runHealthcheck() int {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck: configuration:", err)
		return 1
	}
	baseURL, err := config.HealthcheckURL(cfg.HTTPAddr)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	if err := probe(ctx, baseURL+"/healthz", http.DefaultClient); err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	return 0
}

func probe(ctx context.Context, url string, hc *http.Client) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	resp, err := hc.Do(req)
	if err != nil {
		return fmt.Errorf("health request failed: %w", err)
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 4<<10))
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("health returned HTTP %d", resp.StatusCode)
	}
	return nil
}

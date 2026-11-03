// Healthcheck for the distroless voice-proxy container.
//
// `voice-proxy -healthcheck` loads the same configuration as the server, so a
// broken environment or an unreadable secret file fails the deploy probe, then
// asks Hermes for /v1/models on the configured upstream. No secret ever appears
// on a command line.
package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/config"
)

const healthcheckTimeout = 4 * time.Second

func runHealthcheck() int {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck: configuration:", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	if err := probeUpstream(ctx, cfg.Upstream); err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	return 0
}

// probeUpstream is the liveness assertion: Hermes must answer its model list.
// The probe carries no credentials (real requests forward the caller's
// Authorization), so 401/403 from an API-key-protected Hermes still proves the
// gateway is up; only a transport error or a 5xx is unhealthy.
func probeUpstream(ctx context.Context, baseURL string) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, baseURL+"/v1/models", nil)
	if err != nil {
		return fmt.Errorf("build request: %w", err)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return fmt.Errorf("upstream is unreachable")
	}
	defer func() { _ = resp.Body.Close() }()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 4<<10))
	if resp.StatusCode >= http.StatusInternalServerError {
		return fmt.Errorf("upstream returned HTTP %d", resp.StatusCode)
	}
	return nil
}

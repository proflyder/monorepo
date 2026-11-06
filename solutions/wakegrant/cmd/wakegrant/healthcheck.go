// Healthcheck for the distroless wakegrant container.
//
// `wakegrant -healthcheck` loads the same configuration as the server (token
// and machines Secret files), so a broken mount fails the deploy probe, then
// calls /healthz on loopback. No secret ever appears on a command line.
package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"github.com/proflyder/monorepo/solutions/wakegrant/internal/config"
)

const healthcheckTimeout = 4 * time.Second

func runHealthcheck() int {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck: configuration:", err)
		return 1
	}
	baseURL, err := config.HealthcheckURL(cfg.Listen)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, baseURL+"/healthz", nil)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck: health request failed")
		return 1
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 4<<10))
	if resp.StatusCode != http.StatusOK {
		fmt.Fprintf(os.Stderr, "healthcheck: health returned HTTP %d\n", resp.StatusCode)
		return 1
	}
	return 0
}

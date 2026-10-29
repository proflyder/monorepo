// Healthcheck for the ha-bridge container.
//
// The bridge has no listener, so readiness means "the authenticated scheduler
// API answers and reports a healthy tick loop". The probe runs
// `ha-bridge -healthcheck`; the bearer token is loaded from its token file by
// the normal config loader and never appears in process arguments or probe
// events.
package main

import (
	"context"
	"fmt"
	"io"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/config"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/scheduler"
)

const healthcheckTimeout = 5 * time.Second

// runHealthcheck returns a process exit code.
func runHealthcheck(getenv func(string) string, out io.Writer) int {
	cfg, err := config.Load(getenv)
	if err != nil {
		fmt.Fprintln(out, "healthcheck: configuration:", err)
		return 1
	}
	hc, err := httpapi.NewClient(cfg.SchedulerCAFile, healthcheckTimeout)
	if err != nil {
		fmt.Fprintln(out, "healthcheck: scheduler HTTP client:", err)
		return 1
	}
	client := scheduler.New(cfg.SchedulerURL, cfg.SchedulerToken, hc)

	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	status, err := client.GetStatus(ctx)
	if err != nil {
		fmt.Fprintln(out, "healthcheck: scheduler GetStatus:", err)
		return 1
	}
	if !status.Ready {
		fmt.Fprintln(out, "healthcheck: scheduler is not ready")
		return 1
	}
	return 0
}

// Command alarm runs the same durable scheduler engine as //solutions/scheduler
// and, in addition, serves the legacy ricardo.v1.AlarmService facade. There is
// one process and one SQLite database: the legacy alarm API is a view over the
// shared engine, not a second scheduler.
package main

import (
	"context"
	"log/slog"
	"os"
	"os/signal"
	"syscall"
	_ "time/tzdata" // embed IANA data for minimal hosts

	"ricardo.local/monorepo/solutions/scheduler/app"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := app.Run(ctx, "alarm"); err != nil {
		slog.Error("alarm service stopped", "error", err)
		os.Exit(1)
	}
}

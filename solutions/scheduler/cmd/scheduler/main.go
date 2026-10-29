// Command scheduler runs the durable scheduler service.
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
	if len(os.Args) > 1 && (os.Args[1] == "-healthcheck" || os.Args[1] == "--healthcheck") {
		os.Exit(runHealthcheck())
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := app.Run(ctx, "scheduler"); err != nil {
		slog.Error("scheduler stopped", "error", err)
		os.Exit(1)
	}
}

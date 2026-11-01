// Command mcp-gateway runs the extensible MCP infra gateway.
package main

import (
	"context"
	"log/slog"
	"os"
	"os/signal"
	"syscall"

	"ricardo.local/monorepo/solutions/mcp-gateway/app"
)

func main() {
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "-healthcheck", "--healthcheck":
			os.Exit(runHealthcheck())
		}
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := app.Run(ctx); err != nil {
		slog.Error("mcp-gateway stopped", "error", err)
		os.Exit(1)
	}
}

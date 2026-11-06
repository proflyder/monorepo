// Command wakegrant issues and redeems one-time logon grants for a machine
// woken by Home Assistant. Protocol: packages/contracts/wakegrant/PROTOCOL.md.
package main

import (
	"context"
	"log/slog"
	"os"
	"os/signal"
	"syscall"

	"github.com/proflyder/monorepo/solutions/wakegrant/app"
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
		slog.Error("wakegrant stopped", "error", err)
		os.Exit(1)
	}
}

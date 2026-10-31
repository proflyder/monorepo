// Command currency-bot runs the currency Telegram bot and its internal API.
package main

import (
	"context"
	"log/slog"
	"os"
	"os/signal"
	"syscall"

	"ricardo.local/monorepo/solutions/currency-bot/internal/app"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := app.Run(ctx, "currency-bot"); err != nil {
		slog.Error("currency-bot stopped", "error", err)
		os.Exit(1)
	}
}

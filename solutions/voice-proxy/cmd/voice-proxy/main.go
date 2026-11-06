// Command voice-proxy is the Hermes voice proxy:
// an OpenAI-compatible passthrough that makes replies safe for Piper TTS.
//
// The container is distroless, so the two diagnostics that used to be
// `kubectl exec … sh -c 'curl …'` are subcommands:
//
//	voice-proxy -healthcheck   config + upstream /v1/models probe
//	voice-proxy -status        redacted JSON summary, never a token
package main

import (
	"context"
	"log/slog"
	"os"
	"os/signal"
	"syscall"

	"github.com/proflyder/monorepo/solutions/voice-proxy/app"
)

func main() {
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "-healthcheck", "--healthcheck":
			os.Exit(runHealthcheck())
		case "-status", "--status":
			os.Exit(runStatus())
		}
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := app.Run(ctx); err != nil {
		slog.Error("voice-proxy stopped", "error", err)
		os.Exit(1)
	}
}

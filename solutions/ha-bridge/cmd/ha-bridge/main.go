// Command ha-bridge is the Home Assistant executor for the durable scheduler.
//
// Default mode is dry-run: it describes the configuration, reads scheduler
// status and optionally probes Home Assistant, without leasing, executing or
// acknowledging anything. Set BRIDGE_EXECUTE=true for the real executor loop.
package main

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"

	"log/slog"

	"github.com/proflyder/monorepo/packages/go/ha"
	"github.com/proflyder/monorepo/packages/go/httpapi"
	"github.com/proflyder/monorepo/packages/go/scheduler"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/bridge"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/config"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/ledger"
)

func main() {
	if len(os.Args) > 1 && (os.Args[1] == "-healthcheck" || os.Args[1] == "--healthcheck") {
		os.Exit(runHealthcheck(os.Getenv, os.Stderr))
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	if err := run(ctx, os.Getenv, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "ha-bridge:", err)
		os.Exit(1)
	}
}

func run(ctx context.Context, getenv func(string) string, out io.Writer) error {
	cfg, err := config.Load(getenv)
	if err != nil {
		return err
	}

	logger := slog.New(slog.NewTextHandler(out, &slog.HandlerOptions{Level: slog.LevelInfo}))

	schedulerHTTP, err := httpapi.NewClient(cfg.SchedulerCAFile, cfg.SchedulerTimeout)
	if err != nil {
		return fmt.Errorf("scheduler HTTP client: %w", err)
	}
	schedulerClient := scheduler.New(cfg.SchedulerURL, cfg.SchedulerToken, schedulerHTTP)

	var haClient *ha.Client
	if cfg.HAToken != "" {
		haHTTP, err := httpapi.NewClient(cfg.HACAFile, cfg.HATimeout)
		if err != nil {
			return fmt.Errorf("home assistant HTTP client: %w", err)
		}
		haClient = ha.New(cfg.HAURL, cfg.HAToken, haHTTP)
	}

	var receipts *ledger.Ledger
	if cfg.Execute {
		if cfg.LedgerPath == "" {
			logger.Warn("BRIDGE_LEDGER_PATH is unset; duplicate suppression lasts only for this process")
			receipts = ledger.NewMemory()
		} else {
			receipts, err = ledger.Open(cfg.LedgerPath)
			if err != nil {
				return err
			}
		}
	}

	runner := bridge.New(*cfg, schedulerClient, controller(haClient), receipts, logger)

	if err := runner.Run(ctx); err != nil {
		return err
	}
	if cfg.Execute {
		logger.Info("ha-bridge stopped")
	}
	return nil
}

// controller adapts the concrete HA client to the bridge interface and keeps
// the bridge testable without network fakes.
func controller(client *ha.Client) bridge.HAController {
	if client == nil {
		return nil
	}
	return client
}

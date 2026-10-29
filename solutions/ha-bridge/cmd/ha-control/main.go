// Command ha-control serves the phone-facing control catalog and executes
// control commands that the phone enqueues through the scheduler.
//
// It is separate from the alarm executor (ha-bridge) on purpose: ha-bridge has
// no listener and only runs schedule events, while ha-control exposes a
// read-only authenticated catalog and handles the dedicated "control" queue.
package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalogapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/control"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/discovery"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/ha"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/scheduler"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := run(ctx, os.Getenv, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "ha-control:", err)
		os.Exit(1)
	}
}

func run(ctx context.Context, getenv func(string) string, out io.Writer) error {
	cfg, err := loadSettings(getenv)
	if err != nil {
		return err
	}
	logger := slog.New(slog.NewTextHandler(out, &slog.HandlerOptions{Level: slog.LevelInfo}))

	haHTTP, err := httpapi.NewClient(cfg.HACAFile, cfg.HATimeout)
	if err != nil {
		return fmt.Errorf("home assistant HTTP client: %w", err)
	}
	haClient := ha.New(cfg.HAURL, cfg.HAToken, haHTTP)
	provider := discovery.New(haClient, catalog.Rules{Label: cfg.Label, Exclude: cfg.Exclude}, cfg.CacheTTL, logger)

	server := &http.Server{
		Addr:              cfg.Listen,
		Handler:           catalogapi.Handler(provider, cfg.ControlToken, logger),
		ReadHeaderTimeout: 5 * time.Second,
	}
	errc := make(chan error, 2)
	go func() {
		logger.Info("catalog listening", "addr", cfg.Listen, "label", cfg.Label, "exclude", cfg.Exclude, "execute", cfg.Execute)
		if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			errc <- err
		}
	}()

	if cfg.Execute {
		schedHTTP, err := httpapi.NewClient(cfg.SchedCA, cfg.SchedulerTimeout)
		if err != nil {
			return fmt.Errorf("scheduler HTTP client: %w", err)
		}
		executor := control.New(control.Config{
			ConsumerID:   cfg.ConsumerID,
			Queue:        cfg.Queue,
			BatchSize:    cfg.BatchSize,
			LeaseSeconds: cfg.LeaseSeconds,
			PollInterval: cfg.PollInterval,
			MaxBackoff:   cfg.MaxBackoff,
			MaxAttempts:  cfg.MaxAttempts,
		}, provider, scheduler.New(cfg.SchedulerURL, cfg.SchedulerToken, schedHTTP), haClient, logger)
		go func() { errc <- executor.Run(ctx) }()
	} else {
		logger.Warn("CONTROL_EXECUTE is not true: serving the catalog only; commands are not executed")
	}

	select {
	case <-ctx.Done():
	case err := <-errc:
		if err != nil {
			return err
		}
	}
	shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	return server.Shutdown(shutdown)
}

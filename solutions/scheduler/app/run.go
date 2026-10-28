// Package app wires configuration, store, engine and the API server. It is
// shared by the scheduler and alarm binaries so both expose the same durable
// engine and contract.
package app

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"

	"ricardo.local/monorepo/solutions/scheduler/internal/api"
	"ricardo.local/monorepo/solutions/scheduler/internal/config"
	"ricardo.local/monorepo/solutions/scheduler/internal/engine"
	"ricardo.local/monorepo/solutions/scheduler/internal/store"
)

// Version is reported by GetStatus.
const Version = "0.1.0"

// Run starts the service and blocks until ctx is cancelled or serving fails.
func Run(ctx context.Context, name string) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	logger := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelInfo}))

	st, err := store.Open(ctx, cfg.DBPath, store.Config{
		DefaultTimezone:      cfg.Timezone,
		MissedGrace:          cfg.MissedGrace,
		RingWindow:           cfg.RingWindow,
		MaxTimers:            cfg.MaxTimers,
		MaxTimerDuration:     cfg.MaxTimerDuration,
		MaxPayloadBytes:      cfg.MaxPayloadBytes,
		MaxAttempts:          cfg.MaxAttempts,
		EventRetention:       cfg.EventRetention,
		ConsumerLiveness:     90 * time.Second,
		MaxLease:             time.Hour,
		DefaultSnoozeMinutes: 9,
	})
	if err != nil {
		return err
	}
	defer func() { _ = st.Close() }()

	eng := engine.New(st, engine.Options{
		Now:          time.Now,
		TickInterval: cfg.TickInterval,
		Logger:       logger,
	})
	engineCtx, stopEngine := context.WithCancel(ctx)
	defer stopEngine()
	engineDone := make(chan struct{})
	go func() {
		defer close(engineDone)
		eng.Run(engineCtx)
	}()

	svc := api.NewService(api.Options{
		Store:            st,
		Now:              time.Now,
		Version:          Version,
		Status:           eng.Status,
		AllowedConsumers: cfg.AllowedConsumers,
		Logger:           logger,
	})
	handler := api.NewHTTPHandler(svc, api.NewAlarmService(svc), cfg.Token)
	srv := api.NewHTTPServer(cfg.Listen, handler)

	ln, err := net.Listen("tcp", cfg.Listen)
	if err != nil {
		return err
	}
	logger.Info("scheduler listening",
		"service", name,
		"version", Version,
		"addr", ln.Addr().String(),
		"db", cfg.DBPath,
		"tick", cfg.TickInterval.String(),
		"grace_seconds", int(cfg.MissedGrace.Seconds()),
		"ring_window_seconds", int(cfg.RingWindow.Seconds()),
		"consumers_restricted", len(cfg.AllowedConsumers) > 0,
	)

	serveErr := make(chan error, 1)
	go func() {
		serveErr <- srv.Serve(ln)
	}()

	var runErr error
	select {
	case <-ctx.Done():
	case err := <-serveErr:
		if !errors.Is(err, http.ErrServerClosed) {
			runErr = err
		}
	}

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := srv.Shutdown(shutdownCtx); err != nil && runErr == nil {
		runErr = err
	}
	stopEngine()
	<-engineDone
	if errors.Is(runErr, http.ErrServerClosed) {
		runErr = nil
	}
	return runErr
}

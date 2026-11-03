// Package app is the process composition root: configuration, SQLite grant
// store and the HTTP listener.
package app

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"

	"ricardo.local/monorepo/solutions/wakegrant/internal/config"
	"ricardo.local/monorepo/solutions/wakegrant/internal/wakegrant"
)

// Version is the reported implementation version.
const Version = "0.1.0"

// Run starts the service and blocks until ctx is cancelled or the listener
// fails.
func Run(ctx context.Context) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: logLevel()}))
	slog.SetDefault(logger)

	keys, err := wakegrant.LoadMachinesFile(cfg.MachinesFile)
	if err != nil {
		return err
	}
	store, err := wakegrant.OpenStore(ctx, cfg.DBPath)
	if err != nil {
		return err
	}
	defer func() { _ = store.Close() }()

	service := wakegrant.New(wakegrant.Options{
		Keys:        keys,
		IssuerToken: cfg.IssuerToken,
		Store:       store,
		MaxTTL:      cfg.MaxTTL,
		Logger:      logger,
	})

	httpServer := &http.Server{
		Addr:              cfg.Listen,
		Handler:           service.Handler(),
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       10 * time.Second,
		WriteTimeout:      10 * time.Second,
		IdleTimeout:       time.Minute,
	}
	ln, err := net.Listen("tcp", cfg.Listen)
	if err != nil {
		return err
	}
	logger.Info("wakegrant listening",
		"service", "wakegrant",
		"version", Version,
		"listen", ln.Addr().String(),
		"db_path", cfg.DBPath,
		"machines", len(keys),
		"max_ttl", cfg.MaxTTL.String(),
	)

	serveErr := make(chan error, 1)
	go func() { serveErr <- httpServer.Serve(ln) }()

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
	if err := httpServer.Shutdown(shutdownCtx); err != nil && runErr == nil {
		runErr = fmt.Errorf("shutdown: %w", err)
	}
	logger.Info("wakegrant stopped", "event", "stopped")
	return runErr
}

func logLevel() slog.Level {
	switch os.Getenv("LOG_LEVEL") {
	case "debug", "DEBUG":
		return slog.LevelDebug
	case "warn", "WARN":
		return slog.LevelWarn
	case "error", "ERROR":
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

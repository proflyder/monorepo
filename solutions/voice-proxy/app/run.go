// Package app is the process composition root: configuration, the upstream
// client and the HTTP listener.
//
// This step wires only the core proxy. The voice router, the dispatcher
// updates poller and the durable queue consumer are added in the following
// steps; the comments in Run() mark where they attach so the listener,
// start-up order and shutdown stay in one place.
package app

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/config"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/hermes"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/proxy"
)

// Version is the reported implementation version.
const Version = "0.1.0"

const (
	readHeaderTimeout = 10 * time.Second
	shutdownTimeout   = 10 * time.Second
)

// Run starts the service and blocks until ctx is cancelled or the listener
// fails.
func Run(ctx context.Context) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	logger := proxy.NewStderrLogger(os.Stderr)

	handler := proxy.New(proxy.Options{
		UpstreamURL:     cfg.Upstream,
		ReasoningEffort: cfg.ReasoningEffort,
		MaxBody:         hermes.MaxBody,
		Timeout:         hermes.DefaultTimeout,
		Logger:          logger,
		// Routing (step 3): set Router to a *router.Router built from cfg.HAURL,
		// cfg.HAToken, cfg.GatewayURL and cfg.GatewayToken when
		// cfg.RoutingEnabled(); leave nil otherwise. Routing stays off for a
		// partially configured deployment, exactly like routing_enabled().
		Router: nil,
	})

	server := &http.Server{
		Addr:              cfg.Listen(),
		Handler:           handler,
		ReadHeaderTimeout: readHeaderTimeout,
		// No WriteTimeout: a buffered turn may take the full 180 s upstream
		// timeout, and streaming tunes its own deadlines.
	}
	listener, err := net.Listen("tcp", cfg.Listen())
	if err != nil {
		return err
	}
	logger.Infof("hermes voice proxy on http://%s:%d -> %s", cfg.Bind, cfg.Port, cfg.Upstream)

	// Background workers (later steps) must be started after the listener is
	// bound and stopped *before* the server closes, in this order:
	//   1. dispatcher updates poller   when cfg.UpdatesEnabled()
	//   2. result announcer / router   when cfg.RoutingEnabled()
	//   3. queue consumer              when cfg.QueueEnabledAndRouted()
	// Each one takes a context derived from ctx, so the deferred Shutdown below
	// stops accepting requests while the workers drain.

	serveErr := make(chan error, 1)
	go func() { serveErr <- server.Serve(listener) }()

	var runErr error
	select {
	case <-ctx.Done():
	case err := <-serveErr:
		if !errors.Is(err, http.ErrServerClosed) {
			runErr = err
		}
	}

	shutdownCtx, cancel := context.WithTimeout(context.Background(), shutdownTimeout)
	defer cancel()
	if err := server.Shutdown(shutdownCtx); err != nil && runErr == nil {
		runErr = fmt.Errorf("shutdown: %w", err)
	}
	logger.Infof("stopped")
	return runErr
}

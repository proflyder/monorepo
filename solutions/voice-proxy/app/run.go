// Package app is the process composition root: configuration, the upstream
// client, the voice router, the background workers and the HTTP listener.
//
// Start-up order:
//
//   - the listener is bound first;
//   - the dispatcher updates poller starts when routing and updates are on;
//   - the queue consumer starts only when the queue and routing are both on;
//   - the router exists only when the full HA + gateway path is configured, so
//     a partially configured deployment keeps the plain sanitiser behaviour.
//
// Shutdown stops accepting requests first, then cancels the workers and joins
// them, so an in-flight request can still start its result announcer.
package app

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"sync"
	"time"

	"ricardo.local/monorepo/packages/go/httpapi"
	"ricardo.local/monorepo/packages/go/scheduler"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/announce"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/config"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/gateway"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/hermes"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/proxy"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/queue"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/result"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/router"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/updates"
)

// Version is the reported implementation version.
const Version = "0.2.0"

const (
	readHeaderTimeout = 10 * time.Second
	shutdownTimeout   = 10 * time.Second
	workerTimeout     = 5 * time.Second
)

// Run starts the service and blocks until ctx is cancelled or the listener
// fails.
func Run(ctx context.Context) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	logger := proxy.NewStderrLogger(os.Stderr)
	httpClient, err := httpapi.NewClient("", 0)
	if err != nil {
		return err
	}

	workerCtx, stopWorkers := context.WithCancel(ctx)
	defer stopWorkers()
	var workers sync.WaitGroup
	startWorker := func(run func(context.Context)) {
		workers.Add(1)
		go func() {
			defer workers.Done()
			run(workerCtx)
		}()
	}

	var routerImpl proxy.Router
	var results *result.Announcer
	var poller *updates.Poller
	var consumer *queue.Consumer

	if cfg.RoutingEnabled() {
		reader := profile.New(profile.Config{
			URL:     cfg.HAURL,
			Token:   cfg.HATokenValue,
			Entity:  cfg.ProfileEntity,
			Cache:   cfg.ProfileCache,
			Timeout: cfg.HATimeout,
			HTTP:    httpClient,
		})
		gateways := gateway.New(gateway.Config{
			URL:     cfg.GatewayURL,
			Token:   cfg.GatewayTokenValue,
			Timeout: cfg.GatewayTimeout,
			HTTP:    httpClient,
		})
		announcer := announce.New(announce.Config{
			URL:       cfg.HAURL,
			Token:     cfg.HATokenValue,
			Script:    cfg.AnnounceScript,
			Satellite: cfg.Satellite,
			Timeout:   cfg.HATimeout,
			HTTP:      httpClient,
		})

		// The durable producer writes with its own scheduler URL and token; the
		// queue switch alone decides whether it is usable.
		var producer *queue.Producer
		var schedulerClient *scheduler.Client
		if cfg.QueueEnabled() || cfg.SchedulerURL != "" {
			schedulerClient = scheduler.New(cfg.SchedulerURL, cfg.SchedulerTokenValue(), httpClient)
			producer = queue.NewProducer(schedulerClient, cfg.QueueEnabled)
		}
		var enqueue result.EnqueueFunc
		var updatesEnqueue updates.EnqueueFunc
		if producer != nil {
			enqueue = func(ctx context.Context, speech, profileID, threadID, seq string, createdUnix *int64) bool {
				callCtx, cancel := context.WithTimeout(ctx, cfg.GatewayTimeout)
				defer cancel()
				return producer.Enqueue(callCtx, speech, profileID, threadID, seq, createdUnix)
			}
			updatesEnqueue = updates.EnqueueFunc(enqueue)
		}

		results = result.New(result.Config{
			Context:      workerCtx,
			Client:       gateways,
			Announce:     announcer,
			Enqueue:      enqueue,
			QueueEnabled: cfg.QueueEnabled,
			MaxChars:     cfg.AnnounceMaxChars,
			Timeout:      cfg.ResultTimeout,
			Wait:         cfg.ResultWait,
			Retry:        cfg.ResultRetry,
			Logger:       logger,
		})
		routerImpl = router.New(router.Config{
			Profile: reader,
			Gateway: gateways,
			Results: results,
		})

		if cfg.UpdatesEnabled() {
			poller = updates.New(updates.Config{
				Fetch:        gateways.Updates,
				Profile:      reader.Fetch,
				Announce:     announcer.Announce,
				Enqueue:      updatesEnqueue,
				QueueEnabled: cfg.QueueEnabled,
				Rescan:       cfg.UpdatesRescan,
				Wait:         cfg.ResultWait,
				MaxChars:     cfg.AnnounceMaxChars,
				TTL:          cfg.AnnounceTTL,
				RetryMin:     cfg.UpdatesRetry,
			})
		}

		if cfg.QueueEnabledAndRouted() && schedulerClient != nil {
			consumer = queue.NewConsumer(queue.ConsumerConfig{
				Profile:     reader.Fetch,
				State:       reader.State,
				List:        listDue(schedulerClient, cfg.GatewayTimeout),
				Ack:         ackEvent(schedulerClient, cfg.GatewayTimeout),
				Announce:    announcer.Announce,
				ConsumerID:  cfg.QueueConsumer,
				Satellite:   cfg.Satellite,
				MediaPlayer: cfg.MediaPlayer,
				TTL:         cfg.AnnounceTTL,
				Logger:      logger,
			})
		}
	}

	handler := proxy.New(proxy.Options{
		UpstreamURL:     cfg.Upstream,
		ReasoningEffort: cfg.ReasoningEffort,
		MaxBody:         hermes.MaxBody,
		Timeout:         hermes.DefaultTimeout,
		Logger:          logger,
		Router:          routerImpl,
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
	if routerImpl != nil {
		logger.Infof("voice routing enabled profile=%s", cfg.ProfileEntity)
	}
	if poller != nil {
		logger.Infof("updates poller enabled rescan=%d", cfg.UpdatesRescan)
		startWorker(poller.Run)
	}
	if consumer != nil {
		logger.Infof("queue consumer enabled consumer=%s", cfg.QueueConsumer)
		startWorker(consumer.Run)
	}

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

	// Stop the background workers after the listener drained: a request that is
	// still finishing may start its result announcer.
	stopWorkers()
	done := make(chan struct{})
	go func() {
		workers.Wait()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(workerTimeout):
	}
	if results != nil {
		results.Wait(workerTimeout)
	}
	logger.Infof("stopped")
	return runErr
}

// listDue adapts the scheduler client to the consumer, bounding each call with
// the gateway timeout like _scheduler_rpc.
func listDue(client *scheduler.Client, timeout time.Duration) func(ctx context.Context, queueName, consumer string, leaseSeconds, limit int) ([]scheduler.Event, error) {
	return func(ctx context.Context, queueName, consumerID string, leaseSeconds, limit int) ([]scheduler.Event, error) {
		callCtx, cancel := context.WithTimeout(ctx, timeout)
		defer cancel()
		return client.ListDueEvents(callCtx, scheduler.ListRequest{
			ConsumerID:   consumerID,
			Queue:        queueName,
			LeaseSeconds: leaseSeconds,
			Limit:        limit,
		})
	}
}

// ackEvent adapts AckEvent with the same per-call timeout.
func ackEvent(client *scheduler.Client, timeout time.Duration) func(ctx context.Context, eventID, consumer, outcome, detail string) error {
	return func(ctx context.Context, eventID, consumerID, outcome, detail string) error {
		callCtx, cancel := context.WithTimeout(ctx, timeout)
		defer cancel()
		return client.AckEvent(callCtx, eventID, consumerID, outcome, detail)
	}
}

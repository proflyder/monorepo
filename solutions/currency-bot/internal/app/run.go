// Package app wires configuration, storage, parsing, alerting, HTTP servers
// and the two periodic jobs. It is the process composition root.
package app

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"
	_ "time/tzdata" // embed IANA data for minimal hosts

	"ricardo.local/monorepo/solutions/currency-bot/internal/alerts"
	"ricardo.local/monorepo/solutions/currency-bot/internal/config"
	"ricardo.local/monorepo/solutions/currency-bot/internal/metrics"
	"ricardo.local/monorepo/solutions/currency-bot/internal/rates"
	"ricardo.local/monorepo/solutions/currency-bot/internal/server"
	"ricardo.local/monorepo/solutions/currency-bot/internal/service"
	"ricardo.local/monorepo/solutions/currency-bot/internal/store"
	"ricardo.local/monorepo/solutions/currency-bot/internal/telegram"
)

const (
	jobHourly  = "hourly"
	jobCleanup = "cleanup"
)

// Run starts the service and blocks until ctx is cancelled or a listener fails.
func Run(ctx context.Context, name string) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: logLevel()}))
	slog.SetDefault(logger)

	st, err := store.Open(ctx, cfg.DBPath)
	if err != nil {
		return err
	}
	defer func() { _ = st.Close() }()

	m := metrics.New()
	fetcher := rates.NewFetcher(cfg.KursURL, nil, logger)
	checker := alerts.New(st, st, logger, time.Now)

	tg := telegram.New(cfg.TelegramAPI, cfg.BotToken, logger)
	tg.OnSendFailure = m.IncTelegramFailure

	pipeline := &service.Service{
		Source:   fetcher,
		History:  st,
		Checker:  checker,
		Recorder: st,
		Sender:   tg,
		Metrics:  m,
		Logger:   logger,
		Now:      time.Now,
	}

	srv := server.New(server.Options{
		APIToken:      cfg.APIToken,
		WebhookSecret: cfg.WebhookSecret,
		AllowedChats:  cfg.AllowedChats,
		DefaultChatID: cfg.AllowedChats[0],
		Pipeline:      pipeline,
		History:       st,
		Sender:        tg,
		Metrics:       m,
		Logger:        logger,
		Now:           time.Now,
	})

	if cfg.WebhookURL != "" {
		registerCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
		if err := tg.SetWebhook(registerCtx, cfg.WebhookURL, cfg.WebhookSecret); err != nil {
			logger.Error("failed to register telegram webhook", "error", err)
		}
		cancel()
	}

	apiSrv := &http.Server{
		Addr:              cfg.HTTPAddr,
		Handler:           srv.APIHandler(),
		ReadHeaderTimeout: 10 * time.Second,
	}
	webhookSrv := &http.Server{
		Addr:              cfg.WebhookAddr,
		Handler:           srv.WebhookHandler(),
		ReadHeaderTimeout: 10 * time.Second,
	}

	apiLn, err := net.Listen("tcp", cfg.HTTPAddr)
	if err != nil {
		return err
	}
	webhookLn, err := net.Listen("tcp", cfg.WebhookAddr)
	if err != nil {
		_ = apiLn.Close()
		return err
	}

	logger.Info("currency-bot listening",
		"service", name,
		"http_addr", apiLn.Addr().String(),
		"webhook_addr", webhookLn.Addr().String(),
		"db", cfg.DBPath,
		"timezone", cfg.Timezone,
		"chats", len(cfg.AllowedChats),
		"hourly_minute", cfg.HourlyMinute,
		"cleanup_day", cfg.CleanupDay.String(),
		"cleanup_hour", cfg.CleanupHour,
		"retention_days", cfg.RetentionDays,
	)

	serveErr := make(chan error, 2)
	go func() { serveErr <- apiSrv.Serve(apiLn) }()
	go func() { serveErr <- webhookSrv.Serve(webhookLn) }()

	jobCtx, stopJobs := context.WithCancel(ctx)
	defer stopJobs()

	go runHourly(jobCtx, cfg.Location, cfg.HourlyMinute, logger, func(jobCtx context.Context) {
		if err := pipeline.Run(jobCtx, jobHourly, cfg.AllowedChats[0], false, true); err != nil {
			logger.Error("hourly job failed", "event", "job_failed", "job", jobHourly, "error", err)
		}
	})
	go runWeekly(jobCtx, cfg.Location, cfg.CleanupDay, cfg.CleanupHour, logger, func(jobCtx context.Context) {
		m.IncJobRun(jobCleanup)
		removed, err := st.CleanOldRecords(jobCtx, time.Duration(cfg.RetentionDays)*24*time.Hour, time.Now())
		if err != nil {
			m.IncJobFailure(jobCleanup)
			logger.Error("cleanup job failed", "event", "job_failed", "job", jobCleanup, "error", err)
			return
		}
		logger.Info("cleanup job completed", "event", "cleanup_completed", "job", jobCleanup, "removed", removed)
	})

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
	if err := apiSrv.Shutdown(shutdownCtx); err != nil && runErr == nil {
		runErr = err
	}
	if err := webhookSrv.Shutdown(shutdownCtx); err != nil && runErr == nil {
		runErr = err
	}
	stopJobs()
	if errors.Is(runErr, http.ErrServerClosed) {
		runErr = nil
	}
	logger.Info("currency-bot stopped", "event", "stopped")
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

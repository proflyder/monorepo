// Package app is the process composition root: configuration, catalog,
// backends, MCP server and HTTP listeners.
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

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/agentmode"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/bb"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/config"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/group"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/loki"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/metrics"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/pending"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/prometheus"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/server"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/telegram"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/tools"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/voice"
)

// Version is the reported implementation version.
const Version = "0.1.0"

const instructions = `Инфраструктурный шлюз домашнего кластера.
Начинай диагностику с infra_status (один агрегированный вызов), затем уточняй
infra_service или infra_logs по имени сервиса из каталога.
Перезапуск выполняется только в два шага: infra_prepare_restart, затем
infra_confirm после подтверждения человеком. Никогда не выдумывай имена:
бери их из infra_overview или ресурса infra://catalog.
Агенты bb: bb_status показывает только личные треды; запуск — только в личные
проекты, в два шага bb_prepare_spawn → bb_confirm, отмена — bb_cancel.
Рабочие проекты и треды недоступны и не обсуждаются.
Отвечай коротко; длинные отчёты отправляй через infra_send_report.`

// Run starts the gateway and blocks until ctx is cancelled or a listener fails.
func Run(ctx context.Context) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: logLevel()}))
	slog.SetDefault(logger)

	cat := &catalog.Catalog{}
	if cfg.CatalogFile != "" {
		cat, err = catalog.LoadFile(cfg.CatalogFile)
		if err != nil {
			return err
		}
	}

	m := metrics.New()
	mode := agentmode.New()
	deps := &tools.Deps{
		Catalog:          cat,
		Pending:          pending.New(cfg.PendingTTL, nil),
		BBPending:        pending.NewKeyed[bb.SpawnAction](cfg.PendingTTL, nil),
		BBMode:           mode,
		Timers:           tools.TimerScheduler{},
		Metrics:          m,
		Logger:           logger,
		Now:              time.Now,
		Location:         cfg.Location,
		Cooldown:         cfg.Cooldown,
		SelfRestartDelay: cfg.SelfRestartDelay,
		QueryTimeout:     cfg.QueryTimeout,
		StatusTimeout:    cfg.StatusTimeout,
		ReportTimeout:    cfg.ReportTimeout,
		LogByteCap:       cfg.LogByteCap,
	}
	if cfg.BBAddress != "" {
		deps.BB = bb.NewDispatcher(&bb.SSHRunner{
			Address:        cfg.BBAddress,
			User:           cfg.BBUser,
			KeyFile:        cfg.BBKeyFile,
			KnownHostsFile: cfg.BBKnownHostsFile,
			Timeout:        cfg.BBTimeout,
		}, cfg.BBTimeout)
	}
	if cfg.BBTargetsFile != "" {
		resolver, err := bb.LoadTargetsFile(cfg.BBTargetsFile)
		if err != nil {
			return err
		}
		deps.BBTargets = resolver
	}
	deps.BBDispatcherThreadID = cfg.BBDispatcherThreadID
	deps.Checks = checks.New(cfg.HAURL, cfg.HAToken, nil).WithTimeout(cfg.QueryTimeout)
	if cfg.PrometheusURL != "" {
		deps.Prom = prometheus.New(cfg.PrometheusURL, nil)
	}
	if cfg.AlertmanagerURL != "" {
		deps.Alerts = alertmanager.New(cfg.AlertmanagerURL, nil)
	}
	if cfg.LokiURL != "" {
		deps.Loki = loki.New(cfg.LokiURL, nil)
	}
	kube, err := kubernetes.New(kubernetes.Options{
		BaseURL:   cfg.KubeAPIURL,
		TokenFile: cfg.KubeTokenFile,
		CAFile:    cfg.KubeCAFile,
	})
	if err != nil {
		return err
	}
	deps.Kube = kube
	if cfg.TelegramToken != "" && cfg.TelegramChatID != "" {
		deps.Report = telegram.New(cfg.TelegramAPIURL, cfg.TelegramToken, cfg.TelegramChatID, nil)
	}

	registry := group.NewRegistry(
		&tools.ReadGroup{Deps: deps},
		&tools.FixGroup{Deps: deps},
		&tools.ReportGroup{Deps: deps},
		&tools.BBReadGroup{Deps: deps},
		&tools.BBActGroup{Deps: deps},
	)
	selected, err := registry.Select(cfg.Groups)
	if err != nil {
		return err
	}

	mcpServer := mcp.NewServer(&mcp.Implementation{
		Name:    "mcp-gateway",
		Title:   "Infra gateway",
		Version: Version,
	}, &mcp.ServerOptions{
		Instructions: instructions,
		Logger:       logger,
	})
	registry.RegisterAll(mcpServer, selected)

	streamable := mcp.NewStreamableHTTPHandler(
		func(*http.Request) *mcp.Server { return mcpServer },
		&mcp.StreamableHTTPOptions{
			SessionTimeout: 15 * time.Minute,
			Logger:         logger,
		},
	)

	var voiceHandler http.Handler
	if cfg.VoiceToken != "" {
		voiceHandler = voice.New(voice.Options{
			Dispatcher:   deps.BB,
			WorkThreadID: cfg.BBDispatcherThreadID,
			Mode:         mode,
			Metrics:      m,
			Logger:       logger,
			Now:          time.Now,
		}).Handler()
	}

	handler := server.New(server.Options{
		AuthToken:  cfg.AuthToken,
		VoiceToken: cfg.VoiceToken,
		MCP:        streamable,
		Voice:      voiceHandler,
		Ready: func(context.Context) error {
			if cat == nil {
				return errors.New("catalog is not loaded")
			}
			return nil
		},
		Metrics: m,
		Logger:  logger,
	})

	httpServer := &http.Server{
		Addr:              cfg.HTTPAddr,
		Handler:           handler.Handler(),
		ReadHeaderTimeout: 10 * time.Second,
		IdleTimeout:       2 * time.Minute,
	}
	ln, err := net.Listen("tcp", cfg.HTTPAddr)
	if err != nil {
		return err
	}

	logger.Info("mcp-gateway listening",
		"service", "mcp-gateway",
		"version", Version,
		"http_addr", ln.Addr().String(),
		"groups", group.EnabledNames(selected),
		"catalog_services", len(cat.Services),
		"prometheus", cfg.PrometheusURL != "",
		"alertmanager", cfg.AlertmanagerURL != "",
		"loki", cfg.LokiURL != "",
		"home_assistant", cfg.HAURL != "",
		"telegram", deps.Report != nil,
		"bb", deps.BB != nil,
		"bb_targets", bbTargetCount(deps.BBTargets),
		"voice", voiceHandler != nil,
		"pending_ttl", cfg.PendingTTL.String(),
		"cooldown", cfg.Cooldown.String(),
		"self_restart_delay", cfg.SelfRestartDelay.String(),
		"timezone", cfg.Timezone,
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
	if errors.Is(runErr, http.ErrServerClosed) {
		runErr = nil
	}
	logger.Info("mcp-gateway stopped", "event", "stopped")
	return runErr
}

func bbTargetCount(resolver *bb.Resolver) int {
	if resolver == nil {
		return 0
	}
	return len(resolver.Entries())
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

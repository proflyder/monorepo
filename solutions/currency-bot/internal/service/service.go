// Package service orchestrates the fetch → save → alert → send pipeline.
package service

import (
	"context"
	"io"
	"log/slog"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/alerts"
	"ricardo.local/monorepo/solutions/currency-bot/internal/message"
	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

// RatesSource fetches the current rates.
type RatesSource interface {
	Fetch(ctx context.Context) (model.Rates, error)
}

// HistoryStore persists history snapshots.
type HistoryStore interface {
	SaveRecord(ctx context.Context, record model.Record) (int, error)
}

// Sender delivers one Telegram message.
type Sender interface {
	Send(ctx context.Context, chatID, text string) error
}

// AlertRecorder writes deduplication rows after a successful send.
type AlertRecorder interface {
	RecordSentAlert(ctx context.Context, alert model.SentAlert) error
}

// Metrics receives pipeline telemetry; all methods are best-effort.
type Metrics interface {
	SetRates(usdBuy, usdSell, rubBuy, rubSell float64)
	ObserveParse(job string, seconds float64)
	IncJobRun(job string)
	IncJobFailure(job string)
	IncTelegramFailure()
}

// Service runs the currency pipeline.
type Service struct {
	Source   RatesSource
	History  HistoryStore
	Checker  *alerts.Checker
	Recorder AlertRecorder
	Sender   Sender
	Metrics  Metrics
	Logger   *slog.Logger
	Now      func() time.Time
}

// Run executes one pipeline invocation. forceNotification skips dedup reads and
// always sends a message; saveToHistory controls whether the snapshot is
// persisted. Threshold-check and history-save failures are logged and ignored,
// matching the Kotlin behaviour.
func (s *Service) Run(ctx context.Context, job, chatID string, forceNotification, saveToHistory bool) error {
	nowFn := s.Now
	if nowFn == nil {
		nowFn = time.Now
	}
	if s.Metrics != nil {
		s.Metrics.IncJobRun(job)
	}
	start := time.Now()
	rates, err := s.Source.Fetch(ctx)
	if s.Metrics != nil {
		s.Metrics.ObserveParse(job, time.Since(start).Seconds())
	}
	if err != nil {
		if s.Metrics != nil {
			s.Metrics.IncJobFailure(job)
		}
		s.Logger.Error("failed to fetch currency rates", "job", job, "error", err)
		return err
	}
	now := nowFn()

	if s.Metrics != nil {
		s.Metrics.SetRates(rates.USDToKZT.Buy, rates.USDToKZT.Sell, rates.RUBToKZT.Buy, rates.RUBToKZT.Sell)
	}
	s.logRate("USD", "KZT", rates.USDToKZT, now)
	s.logRate("RUB", "KZT", rates.RUBToKZT, now)

	if saveToHistory {
		if _, err := s.History.SaveRecord(ctx, model.Record{Timestamp: now, Rates: rates}); err != nil {
			s.Logger.Error("failed to save history", "error", err)
		}
	} else {
		s.Logger.Info("skipping history save (manual trigger)")
	}

	var candidates []model.Alert
	if s.Checker != nil {
		candidates, err = s.Checker.Check(ctx, rates, forceNotification)
		if err != nil {
			s.Logger.Error("failed to check thresholds", "error", err)
			candidates = nil
		}
	}

	if len(candidates) == 0 && !forceNotification {
		s.Logger.Info("no thresholds exceeded, skipping telegram notification")
		return nil
	}

	text := message.Format(rates, candidates)
	s.Logger.Debug("message content", "text", text)
	if err := s.Sender.Send(ctx, chatID, text); err != nil {
		if s.Metrics != nil {
			s.Metrics.IncTelegramFailure()
			s.Metrics.IncJobFailure(job)
		}
		s.Logger.Error("failed to send message to telegram", "chat_id", chatID, "error", err)
		return err
	}
	for _, alert := range candidates {
		record := model.SentAlert{
			Key:           alert.Key(),
			Level:         alert.Level,
			Direction:     alert.Direction,
			RateAtAlert:   alert.NewRate,
			ChangePercent: alert.ChangePercent,
			SentAt:        now,
		}
		if err := s.Recorder.RecordSentAlert(ctx, record); err != nil {
			s.Logger.Error("failed to record sent alert", "pair", alert.Pair, "error", err)
		}
	}
	return nil
}

func (s *Service) logRate(from, to string, rate model.ExchangeRate, ts time.Time) {
	if s.Logger == nil {
		return
	}
	s.Logger.Info("currency rate updated",
		"event", "currency_rate_updated",
		"currency_from", from,
		"currency_to", to,
		"rate_buy", rate.Buy,
		"rate_sell", rate.Sell,
		"timestamp", ts.UTC().UnixMilli(),
	)
}

// DiscardLogger is a helper for tests and optional components.
func DiscardLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

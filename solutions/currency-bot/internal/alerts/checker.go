// Package alerts evaluates threshold crossings and deduplicates them against
// previously sent alerts.
package alerts

import (
	"context"
	"io"
	"log/slog"
	"math"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

// HistoryStore supplies historical records for threshold lookbacks.
type HistoryStore interface {
	RecordBefore(ctx context.Context, cutoff time.Time) (model.Record, error)
}

// SentAlertStore supplies and clears deduplication rows.
type SentAlertStore interface {
	GetSentAlert(ctx context.Context, key model.AlertKey) (model.SentAlert, error)
	ClearSentAlert(ctx context.Context, key model.AlertKey) error
}

// Checker evaluates all four periods and returns the alerts to send.
type Checker struct {
	History HistoryStore
	Sent    SentAlertStore
	Logger  *slog.Logger
	Now     func() time.Time
}

// New returns a checker with defaults.
func New(history HistoryStore, sent SentAlertStore, logger *slog.Logger, now func() time.Time) *Checker {
	if logger == nil {
		logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	if now == nil {
		now = time.Now
	}
	return &Checker{History: history, Sent: sent, Logger: logger, Now: now}
}

// Check returns the alerts that should be delivered. skipDedup is true for a
// manual trigger: candidates are accepted unconditionally and no dedup rows are
// read or cleared.
func (c *Checker) Check(ctx context.Context, rates model.Rates, skipDedup bool) ([]model.Alert, error) {
	now := c.Now()
	out := make([]model.Alert, 0, len(model.Periods)*4)
	for _, period := range model.Periods {
		historical, err := c.History.RecordBefore(ctx, now.Add(-period.Duration))
		if err != nil {
			c.Logger.Error("failed to get historical data", "period", period.Name, "error", err)
			continue
		}
		checks := []struct {
			pair     model.Pair
			rateType model.RateType
			current  float64
			old      float64
		}{
			{model.USDToKZT, model.Sell, rates.USDToKZT.Sell, historical.Rates.USDToKZT.Sell},
			{model.USDToKZT, model.Buy, rates.USDToKZT.Buy, historical.Rates.USDToKZT.Buy},
			{model.RUBToKZT, model.Sell, rates.RUBToKZT.Sell, historical.Rates.RUBToKZT.Sell},
			{model.RUBToKZT, model.Buy, rates.RUBToKZT.Buy, historical.Rates.RUBToKZT.Buy},
		}
		for _, check := range checks {
			alert := c.evaluate(period, check.pair, check.rateType, check.current, check.old)
			c.process(ctx, alert, period, check.pair, check.rateType, skipDedup, &out)
		}
	}
	if len(out) == 0 {
		c.Logger.Info("no thresholds exceeded")
	} else {
		c.Logger.Info("found threshold alerts", "count", len(out))
	}
	return out, nil
}

func (c *Checker) process(ctx context.Context, candidate *model.Alert, period model.Period, pair model.Pair, rateType model.RateType, skipDedup bool, out *[]model.Alert) {
	key := model.AlertKey{Pair: pair, Period: period.Name, RateType: rateType}
	if candidate == nil {
		if !skipDedup {
			if err := c.Sent.ClearSentAlert(ctx, key); err != nil {
				c.Logger.Error("failed to clear sent alert", "key", key, "error", err)
			}
		}
		return
	}
	if skipDedup {
		*out = append(*out, *candidate)
		return
	}
	lastSent, err := c.Sent.GetSentAlert(ctx, key)
	var last *model.SentAlert
	if err == nil {
		last = &lastSent
	}
	// A read failure is treated as "no previous alert", matching Kotlin's
	// .getOrNull() semantics.
	if shouldSend(*candidate, last, period) {
		*out = append(*out, *candidate)
	} else {
		c.Logger.Debug("suppressing duplicate alert", "pair", pair, "rate_type", rateType, "period", period.Name)
	}
}

// evaluate mirrors checkPairThresholds: inclusive thresholds, direction DOWN
// for zero/NaN, and no guard against a zero historical rate (Infinity wins,
// NaN does not alert).
func (c *Checker) evaluate(period model.Period, pair model.Pair, rateType model.RateType, current, historical float64) *model.Alert {
	changePercent := (current - historical) / historical * 100.0
	absChange := math.Abs(changePercent)
	level := model.Level("")
	switch {
	case absChange >= period.Critical:
		level = model.Critical
	case absChange >= period.Warning:
		level = model.Warning
	default:
		return nil
	}
	direction := model.Down
	if changePercent > 0 {
		direction = model.Up
	}
	c.Logger.Info("threshold alert",
		"level", level, "pair", pair, "rate_type", rateType,
		"change_percent", changePercent, "period", period.Name,
		"threshold", map[model.Level]float64{model.Warning: period.Warning, model.Critical: period.Critical}[level],
	)
	return &model.Alert{
		Level:         level,
		Period:        period,
		Pair:          pair,
		RateType:      rateType,
		Direction:     direction,
		ChangePercent: changePercent,
		OldRate:       historical,
		NewRate:       current,
	}
}

// shouldSend implements the five deduplication rules.
func shouldSend(candidate model.Alert, last *model.SentAlert, period model.Period) bool {
	if last == nil {
		return true
	}
	if last.Level == model.Warning && candidate.Level == model.Critical {
		return true
	}
	if last.Direction != candidate.Direction {
		return true
	}
	diff := math.Abs((candidate.NewRate - last.RateAtAlert) / last.RateAtAlert * 100.0)
	return diff >= period.ReAlert
}

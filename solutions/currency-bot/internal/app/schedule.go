package app

import (
	"context"
	"log/slog"
	"time"
)

// nextHourly returns the next occurrence at minute:00, strictly after now, in
// loc. There is no catch-up: a missed fire is simply skipped.
func nextHourly(now time.Time, loc *time.Location, minute int) time.Time {
	n := now.In(loc)
	candidate := time.Date(n.Year(), n.Month(), n.Day(), n.Hour(), minute, 0, 0, loc)
	if !candidate.After(n) {
		candidate = candidate.Add(time.Hour)
	}
	return candidate
}

// nextWeekly returns the next occurrence of weekday at hour:00:00 in loc,
// strictly after now.
func nextWeekly(now time.Time, loc *time.Location, weekday time.Weekday, hour int) time.Time {
	n := now.In(loc)
	candidate := time.Date(n.Year(), n.Month(), n.Day(), hour, 0, 0, 0, loc)
	for candidate.Weekday() != weekday || !candidate.After(n) {
		candidate = candidate.AddDate(0, 0, 1)
	}
	return candidate
}

func runHourly(ctx context.Context, loc *time.Location, minute int, logger *slog.Logger, job func(context.Context)) {
	for {
		next := nextHourly(time.Now(), loc, minute)
		timer := time.NewTimer(time.Until(next))
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
			logger.Info("hourly job started", "event", "job_started", "job", jobHourly)
			job(ctx)
		}
	}
}

func runWeekly(ctx context.Context, loc *time.Location, weekday time.Weekday, hour int, logger *slog.Logger, job func(context.Context)) {
	for {
		next := nextWeekly(time.Now(), loc, weekday, hour)
		timer := time.NewTimer(time.Until(next))
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
			logger.Info("cleanup job started", "event", "job_started", "job", jobCleanup)
			job(ctx)
		}
	}
}

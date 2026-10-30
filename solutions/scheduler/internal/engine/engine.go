// Package engine runs the durable tick loop and exposes an honest readiness
// signal. It performs no audio or network side effects: it only advances
// schedules and timers into the durable event outbox.
package engine

import (
	"context"
	"log/slog"
	"sync"
	"time"

	"ricardo.local/monorepo/solutions/scheduler/internal/store"
)

// Status is the last observed tick state.
type Status struct {
	LastTick    time.Time
	LastAttempt time.Time
	LastErrorAt time.Time
	LastError   string
	Ready       bool
}

// Options configures the loop.
type Options struct {
	Now           func() time.Time
	TickInterval  time.Duration
	PurgeInterval time.Duration
	Logger        *slog.Logger
}

// Engine owns the tick goroutine.
type Engine struct {
	st    *store.Store
	now   func() time.Time
	tick  time.Duration
	purge time.Duration
	log   *slog.Logger
	wake  chan struct{}

	mu          sync.RWMutex
	lastTick    time.Time
	lastAttempt time.Time
	lastErrAt   time.Time
	lastErr     string
}

// New builds an engine. Defaults: 1s tick, 1h purge.
func New(st *store.Store, opts Options) *Engine {
	now := opts.Now
	if now == nil {
		now = time.Now
	}
	tick := opts.TickInterval
	if tick <= 0 {
		tick = time.Second
	}
	purge := opts.PurgeInterval
	if purge <= 0 {
		purge = time.Hour
	}
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}
	return &Engine{
		st:    st,
		now:   now,
		tick:  tick,
		purge: purge,
		log:   logger,
		wake:  make(chan struct{}, 1),
	}
}

// Wake requests an immediate tick (for example after starting a timer).
func (e *Engine) Wake() {
	select {
	case e.wake <- struct{}{}:
	default:
	}
}

// Run blocks until ctx is done.
func (e *Engine) Run(ctx context.Context) {
	timer := time.NewTimer(e.tick)
	defer timer.Stop()
	// Tick once immediately so readiness and due work do not wait a full
	// interval after startup.
	if err := e.Tick(ctx); err != nil && ctx.Err() == nil {
		e.log.Error("scheduler initial tick failed", "error", err)
	}
	lastPurge := e.now()
	for {
		select {
		case <-ctx.Done():
			return
		case <-e.wake:
		case <-timer.C:
		}
		if err := e.Tick(ctx); err != nil {
			if ctx.Err() != nil {
				return
			}
			e.log.Error("scheduler tick failed", "error", err)
		}
		if now := e.now(); now.Sub(lastPurge) >= e.purge {
			if n, err := e.st.Purge(ctx, now); err != nil && ctx.Err() == nil {
				e.log.Error("scheduler purge failed", "error", err)
			} else if n > 0 {
				e.log.Info("scheduler purged terminal events", "events", n)
			}
			lastPurge = now
		}
		if !timer.Stop() {
			select {
			case <-timer.C:
			default:
			}
		}
		timer.Reset(e.tick)
	}
}

// Tick runs one engine pass. Failures are recorded in Status so GetStatus
// cannot mask a broken loop.
func (e *Engine) Tick(ctx context.Context) error {
	now := e.now().UTC()
	e.mu.Lock()
	e.lastAttempt = now
	e.mu.Unlock()

	stats, err := e.st.Tick(ctx, now)
	if err != nil {
		e.mu.Lock()
		e.lastErrAt = now
		e.lastErr = truncate(err.Error(), 200)
		e.mu.Unlock()
		return err
	}
	e.mu.Lock()
	e.lastTick = now
	e.lastErr = ""
	e.lastErrAt = time.Time{}
	e.mu.Unlock()
	if stats.Fired > 0 || stats.Skipped > 0 || stats.Timers > 0 || stats.Expired > 0 || stats.LeasesReleased > 0 {
		e.log.Info("scheduler tick",
			"fired", stats.Fired,
			"skipped", stats.Skipped,
			"timers", stats.Timers,
			"expired", stats.Expired,
			"leases_released", stats.LeasesReleased,
		)
	}
	return nil
}

// Status reports readiness: a successful tick within three intervals and no
// newer failure.
func (e *Engine) Status() Status {
	now := e.now()
	e.mu.RLock()
	st := Status{
		LastTick:    e.lastTick,
		LastAttempt: e.lastAttempt,
		LastErrorAt: e.lastErrAt,
		LastError:   e.lastErr,
	}
	e.mu.RUnlock()
	window := 3 * e.tick
	if window < 5*time.Second {
		window = 5 * time.Second
	}
	// Require an explicit success flag: with an injected/frozen clock a failure
	// can carry the same timestamp as the previous success, so comparing times
	// alone would incorrectly report readiness.
	st.Ready = !st.LastTick.IsZero() && now.Sub(st.LastTick) <= window && st.LastError == ""
	return st
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n]
}

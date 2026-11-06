package app

import (
	"context"
	"errors"
	"log/slog"
	"sync"
	"time"
)

var errWebhookNotRegistered = errors.New("telegram webhook is not registered")

// webhookStatus tracks whether the webhook registration succeeded. When no
// WEBHOOK_URL is configured the check is disabled and readiness ignores it.
type webhookStatus struct {
	required bool

	mu         sync.RWMutex
	registered bool
}

// newWebhookStatus returns a status that is already satisfied when no webhook
// URL is configured.
func newWebhookStatus(required bool) *webhookStatus {
	return &webhookStatus{required: required, registered: !required}
}

func (s *webhookStatus) markRegistered() {
	s.mu.Lock()
	s.registered = true
	s.mu.Unlock()
}

// ready returns nil once the webhook is registered (or not required).
func (s *webhookStatus) ready() error {
	s.mu.RLock()
	registered := s.registered
	s.mu.RUnlock()
	if !registered {
		return errWebhookNotRegistered
	}
	return nil
}

// registerWebhook calls set until it succeeds, retrying with exponential
// backoff in [initial, max] until ctx is cancelled. Telegram outages at start-up
// must not crashloop the pod or affect readiness.
func registerWebhook(ctx context.Context, set func(context.Context) error, status *webhookStatus, logger *slog.Logger, initial, max time.Duration) {
	delay := initial
	for {
		attemptCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
		err := set(attemptCtx)
		cancel()
		if err == nil {
			status.markRegistered()
			logger.Info("telegram webhook registered")
			return
		}
		logger.Error("failed to register telegram webhook; retrying", "error", err, "retry_in", delay)
		select {
		case <-ctx.Done():
			return
		case <-time.After(delay):
		}
		if delay < max {
			delay *= 2
			if delay > max {
				delay = max
			}
		}
	}
}

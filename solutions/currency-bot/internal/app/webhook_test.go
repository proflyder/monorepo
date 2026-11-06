package app

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"sync/atomic"
	"testing"
	"time"
)

func discardLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

func TestWebhookStatusReadiness(t *testing.T) {
	disabled := newWebhookStatus(false)
	if err := disabled.ready(); err != nil {
		t.Fatalf("no webhook URL should not affect readiness: %v", err)
	}

	required := newWebhookStatus(true)
	if !errors.Is(required.ready(), errWebhookNotRegistered) {
		t.Fatalf("unregistered webhook must fail readiness, got %v", required.ready())
	}
	required.markRegistered()
	if err := required.ready(); err != nil {
		t.Fatalf("registered webhook must be ready: %v", err)
	}
}

func TestRegisterWebhookRetriesUntilSuccess(t *testing.T) {
	var attempts atomic.Int32
	set := func(context.Context) error {
		if attempts.Add(1) < 3 {
			return errors.New("telegram unavailable")
		}
		return nil
	}
	status := newWebhookStatus(true)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	registerWebhook(ctx, set, status, discardLogger(), time.Millisecond, 2*time.Millisecond)

	if err := status.ready(); err != nil {
		t.Fatalf("webhook should be registered: %v", err)
	}
	if attempts.Load() != 3 {
		t.Fatalf("attempts = %d, want 3", attempts.Load())
	}
}

func TestRegisterWebhookStopsOnContextCancel(t *testing.T) {
	status := newWebhookStatus(true)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	registerWebhook(ctx, func(context.Context) error {
		return errors.New("down")
	}, status, discardLogger(), time.Millisecond, time.Millisecond)
	if err := status.ready(); err == nil {
		t.Fatal("cancelled registration must leave the status unready")
	}
}

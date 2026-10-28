package engine

import (
	"context"
	"path/filepath"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/scheduler/internal/store"
)

func TestReadinessReflectsFailuresOnSameClock(t *testing.T) {
	path := filepath.Join(t.TempDir(), "engine.db")
	st, err := store.Open(context.Background(), path, store.DefaultConfig())
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = st.Close() }()

	frozen := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	eng := New(st, Options{Now: func() time.Time { return frozen }})
	if eng.Status().Ready {
		t.Fatalf("engine ready before first tick")
	}
	if err := eng.Tick(context.Background()); err != nil {
		t.Fatalf("tick: %v", err)
	}
	if !eng.Status().Ready {
		t.Fatalf("engine not ready after successful tick: %+v", eng.Status())
	}

	// Same clock timestamp, now failing: readiness must drop based on the
	// explicit error flag, not a timestamp comparison.
	_ = st.Close()
	if err := eng.Tick(context.Background()); err == nil {
		t.Fatalf("tick after store close succeeded")
	}
	status := eng.Status()
	if status.Ready {
		t.Fatalf("engine reported ready after failure at the same clock instant: %+v", status)
	}
	if status.LastError == "" {
		t.Fatalf("failure not recorded in status")
	}
}

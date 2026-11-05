package api

import (
	"context"
	"net/http"
	"testing"
	"time"

	"ricardo.local/monorepo/packages/go/contracttest"
	"ricardo.local/monorepo/solutions/scheduler/internal/engine"
	"ricardo.local/monorepo/solutions/scheduler/internal/store"
)

const contractToken = "contract-test-token"

// TestContractFixtures replays the golden scheduler wire scenarios in
// testdata/contract. The shared runner owns the HTTP transcript, capture and
// comparison logic; this file only wires the real handler with a fixed clock
// and a temporary SQLite store.
func TestContractFixtures(t *testing.T) {
	contracttest.Run(t, "solutions/scheduler/internal/api/testdata/contract", newContractServer,
		contracttest.WithNormalizer(func(_ string, value any) any {
			// Resource ids are UUIDv7 with a random tail. Event ids captured by
			// a fixture stay readable as {{eventId}}; any other id becomes
			// <uuid>.
			return contracttest.ReplacePattern(value, contracttest.UUIDPattern, "<uuid>")
		}))
}

func newContractServer(t *testing.T, _ string) http.Handler {
	t.Helper()
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	cfg := store.DefaultConfig()
	cfg.MaxAttempts = 2
	st, err := store.Open(context.Background(), t.TempDir()+"/contract.db", cfg)
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })
	svc := NewService(Options{
		Store:   st,
		Now:     func() time.Time { return now },
		Version: "test",
		Status:  func() engine.Status { return engine.Status{Ready: true} },
	})
	return NewHTTPHandler(svc, NewAlarmService(svc), contractToken)
}

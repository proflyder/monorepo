package api

import (
	"context"
	"fmt"
	"net/http"
	"sync"
	"testing"
	"time"

	"github.com/proflyder/monorepo/packages/go/contracttest"
	"github.com/proflyder/monorepo/solutions/scheduler/internal/engine"
	"github.com/proflyder/monorepo/solutions/scheduler/internal/store"
)

const contractToken = "contract-test-token"

var contractBase = time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)

// contractEnv is the mutable fake clock and store behind one scenario. The
// shared runner calls beforeContractStep before every request, so tick-driven
// scenarios can advance the clock and run one engine pass between HTTP steps.
type contractEnv struct {
	mu  sync.Mutex
	now time.Time
	st  *store.Store
}

func (e *contractEnv) Now() time.Time {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.now
}

func (e *contractEnv) SetNow(now time.Time) {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.now = now
}

var contractEnvs sync.Map

// TestContractFixtures replays the golden scheduler wire scenarios in
// testdata/contract. The shared runner owns the HTTP transcript, capture and
// comparison logic; this file wires the real handler with a mutable fake clock
// and a temporary SQLite store.
func TestContractFixtures(t *testing.T) {
	contracttest.Run(t, "solutions/scheduler/internal/api/testdata/contract", newContractServer,
		contracttest.WithBeforeStep(beforeContractStep),
		contracttest.WithNormalizer(func(_ string, value any) any {
			// Resource ids are UUIDv7 with a random tail. Event ids captured by
			// a fixture stay readable as {{eventId}}; any other id becomes
			// <uuid>.
			return contracttest.ReplacePattern(value, contracttest.UUIDPattern, "<uuid>")
		}))
}

func newContractServer(t *testing.T, scenario string) http.Handler {
	t.Helper()
	cfg := store.DefaultConfig()
	cfg.MaxAttempts = 2
	st, err := store.Open(context.Background(), t.TempDir()+"/contract.db", cfg)
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })
	env := &contractEnv{now: contractBase, st: st}
	contractEnvs.Store(scenario, env)
	seedContractScenario(t, env, scenario)

	opts := Options{
		Store:   st,
		Now:     env.Now,
		Version: "test",
		Status:  func() engine.Status { return engine.Status{Ready: true} },
	}
	if scenario == "auth_and_limits" {
		opts.AllowedConsumers = []string{"home-1"}
	}
	svc := NewService(opts)
	return NewHTTPHandler(svc, NewAlarmService(svc), contractToken)
}

func seedContractScenario(t *testing.T, env *contractEnv, scenario string) {
	t.Helper()
	if scenario != "timer_limit" {
		return
	}
	for i := 0; i < 64; i++ {
		if _, err := env.st.StartTimer(context.Background(), fmt.Sprintf("seed-%02d", i), 600, "", contractBase); err != nil {
			t.Fatalf("seed timer %d: %v", i, err)
		}
	}
}

func contractTime(unix int64) time.Time { return time.Unix(unix, 0).UTC() }

// beforeContractStep applies the clock/engine transition of one fixture step.
// Only tick scenarios have transitions; every other scenario keeps the fixed
// clock so its transcript stays deterministic.
func beforeContractStep(t *testing.T, scenario string, step int) {
	t.Helper()
	raw, ok := contractEnvs.Load(scenario)
	if !ok {
		t.Fatalf("no contract environment for scenario %q", scenario)
	}
	env := raw.(*contractEnv)
	switch scenario {
	case "tick_occurrence_fire":
		switch step {
		case 2:
			contractTickAt(t, env, contractTime(1767339000)) // 2026-01-02 07:30 UTC
		case 4:
			contractTickAt(t, env, env.Now())
		}
	case "tick_missed_grace":
		if step == 2 {
			contractTickAt(t, env, contractTime(1767339601)) // due + 601 s
		}
	case "ring_window_expiry_stop":
		switch step {
		case 2:
			contractTickAt(t, env, contractTime(1767339000))
		case 3, 4:
			contractTickAt(t, env, contractTime(1767339901)) // due + 901 s
		}
	case "snooze_flow":
		switch step {
		case 2:
			contractTickAt(t, env, contractTime(1767339000))
		case 7:
			// The 1440-minute snooze is due exactly when the next daily
			// occurrence fires; one tick must coalesce them.
			contractTickAt(t, env, contractTime(1767425400)) // 2026-01-03 07:30 UTC
		}
	case "stop_after_delivered":
		if step == 2 {
			contractTickAt(t, env, contractTime(1767339000))
		}
	case "update_during_ring":
		if step == 2 {
			contractTickAt(t, env, contractTime(1767339000))
		}
	case "timer_lifecycle":
		switch step {
		case 4:
			env.SetNow(contractTime(1767268900)) // base + 100 s
		case 5:
			env.SetNow(contractTime(1767268950)) // base + 150 s
		}
	case "task_lifecycle":
		if step == 9 {
			contractTickAt(t, env, contractTime(1767268861)) // base + 61 s
		}
	case "outbox_lease_expiry":
		switch step {
		case 3:
			env.SetNow(contractTime(1767268831)) // base + 31 s
		case 4:
			env.SetNow(contractTime(1767268862)) // base + 62 s
		}
	}
}

func contractTickAt(t *testing.T, env *contractEnv, now time.Time) {
	t.Helper()
	now = now.UTC()
	env.SetNow(now)
	if _, err := env.st.Tick(context.Background(), now); err != nil {
		t.Fatalf("tick at %s: %v", now, err)
	}
}

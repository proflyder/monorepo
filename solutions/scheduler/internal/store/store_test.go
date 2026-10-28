package store

import (
	"context"
	"database/sql"
	"errors"
	"path/filepath"
	"strings"
	"testing"
	"time"

	_ "modernc.org/sqlite"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

var testBase = time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)

func testConfig() Config {
	c := DefaultConfig()
	c.MissedGrace = 10 * time.Minute
	c.RingWindow = 15 * time.Minute
	c.MaxTimerDuration = time.Hour
	c.MaxPayloadBytes = 1024
	c.MaxAttempts = 2
	c.MaxTimers = 1
	return c
}

func newTestStore(t *testing.T) (*Store, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "scheduler.db")
	st, err := Open(context.Background(), path, testConfig())
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })
	return st, path
}

func reopen(t *testing.T, st *Store, path string) *Store {
	t.Helper()
	_ = st.Close()
	next, err := Open(context.Background(), path, testConfig())
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	t.Cleanup(func() { _ = next.Close() })
	return next
}

func allEvents(t *testing.T, st *Store) []*ricardov1.DueEvent {
	t.Helper()
	rows, err := st.db.QueryContext(context.Background(),
		`SELECT `+eventCols+` FROM events ORDER BY due_unix, id`)
	if err != nil {
		t.Fatalf("query events: %v", err)
	}
	defer func() { _ = rows.Close() }()
	out := make([]*ricardov1.DueEvent, 0)
	for rows.Next() {
		rec, err := scanEvent(rows.Scan)
		if err != nil {
			t.Fatalf("scan event: %v", err)
		}
		out = append(out, rec.event)
	}
	return out
}

func eventCount(t *testing.T, st *Store) int {
	t.Helper()
	return len(allEvents(t, st))
}

func dailyAlarm(hour, minute int) *ricardov1.Schedule {
	return &ricardov1.Schedule{
		Label:    "wake up",
		Kind:     ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM,
		Timezone: "UTC",
		At:       &ricardov1.WallTime{Hour: int32(hour), Minute: int32(minute)},
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_DAILY,
		Enabled:  true,
	}
}

func createAlarm(t *testing.T, st *Store, hour, minute int, now time.Time) *ricardov1.Schedule {
	t.Helper()
	sch, err := st.CreateSchedule(context.Background(), dailyAlarm(hour, minute), "", now)
	if err != nil {
		t.Fatalf("CreateSchedule: %v", err)
	}
	return sch
}

func TestMigrateIsIdempotentAndRejectsNewerSchema(t *testing.T) {
	st, path := newTestStore(t)
	st = reopen(t, st, path)
	var version int
	if err := st.db.QueryRow(`SELECT version FROM schema_meta`).Scan(&version); err != nil {
		t.Fatalf("read version: %v", err)
	}
	if version != SchemaVersion {
		t.Fatalf("version = %d, want %d", version, SchemaVersion)
	}
	_ = st.Close()

	raw, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatalf("open raw: %v", err)
	}
	if _, err := raw.Exec(`UPDATE schema_meta SET version = 99`); err != nil {
		t.Fatalf("bump version: %v", err)
	}
	_ = raw.Close()

	_, err = Open(context.Background(), path, testConfig())
	if err == nil || !strings.Contains(err.Error(), "newer than supported") {
		t.Fatalf("Open newer schema error = %v, want newer-than-supported", err)
	}
}

func TestOpenRequiresPath(t *testing.T) {
	if _, err := Open(context.Background(), "  ", testConfig()); !errors.Is(err, ErrInvalid) {
		t.Fatalf("Open empty path error = %v, want ErrInvalid", err)
	}
}

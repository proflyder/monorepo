// Package store is the durable scheduler domain: SQLite schema, transactions,
// schedule/timer mutations, the occurrence outbox and the tick engine.
//
// Invariants:
//   - exactly one SQLite connection is used, so writes are serialized;
//   - every tick advances a source and inserts its event in one transaction;
//   - event ids are deterministic UUIDv5 over a dedupe identity, so a repeated
//     tick, lease redelivery or process restart cannot duplicate an occurrence;
//   - transport state (pending/leased/delivered) is independent of the active
//     ringing window, so stop/snooze still work after an executor ack.
package store

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

// SchemaVersion is the current SQLite schema version.
const SchemaVersion = 1

// Domain errors mapped to transport status codes by the API layer.
var (
	ErrNotFound       = errors.New("not found")
	ErrConflict       = errors.New("revision conflict")
	ErrIdempotency    = errors.New("idempotency key conflict")
	ErrPrecondition   = errors.New("precondition failed")
	ErrInvalid        = errors.New("invalid argument")
	ErrLimitExceeded  = errors.New("limit exceeded")
	ErrConsumerDenied = errors.New("consumer not allowed")
)

// Config holds operational bounds. Zero values are replaced by
// DefaultConfig values in Open.
type Config struct {
	DefaultTimezone      string
	MissedGrace          time.Duration
	RingWindow           time.Duration
	MaxTimers            int
	MaxTimerDuration     time.Duration
	MaxPayloadBytes      int
	MaxAttempts          int
	DefaultSnoozeMinutes int
	EventRetention       time.Duration
	ConsumerLiveness     time.Duration
	MaxLease             time.Duration
}

// DefaultConfig returns conservative defaults matching the documented policy.
func DefaultConfig() Config {
	return Config{
		DefaultTimezone:      "UTC",
		MissedGrace:          600 * time.Second,
		RingWindow:           900 * time.Second,
		MaxTimers:            64,
		MaxTimerDuration:     7 * 24 * time.Hour,
		MaxPayloadBytes:      16 * 1024,
		MaxAttempts:          5,
		DefaultSnoozeMinutes: 9,
		EventRetention:       30 * 24 * time.Hour,
		ConsumerLiveness:     90 * time.Second,
		MaxLease:             time.Hour,
	}
}

func (c Config) normalized() Config {
	d := DefaultConfig()
	if c.DefaultTimezone == "" {
		c.DefaultTimezone = d.DefaultTimezone
	}
	if c.MissedGrace <= 0 {
		c.MissedGrace = d.MissedGrace
	}
	if c.RingWindow <= 0 {
		c.RingWindow = d.RingWindow
	}
	if c.MaxTimers <= 0 {
		c.MaxTimers = d.MaxTimers
	}
	if c.MaxTimerDuration <= 0 {
		c.MaxTimerDuration = d.MaxTimerDuration
	}
	if c.MaxPayloadBytes <= 0 {
		c.MaxPayloadBytes = d.MaxPayloadBytes
	}
	if c.MaxAttempts <= 0 {
		c.MaxAttempts = d.MaxAttempts
	}
	if c.DefaultSnoozeMinutes <= 0 {
		c.DefaultSnoozeMinutes = d.DefaultSnoozeMinutes
	}
	if c.EventRetention <= 0 {
		c.EventRetention = d.EventRetention
	}
	if c.ConsumerLiveness <= 0 {
		c.ConsumerLiveness = d.ConsumerLiveness
	}
	if c.MaxLease <= 0 {
		c.MaxLease = d.MaxLease
	}
	return c
}

// Store is a SQLite-backed scheduler store.
type Store struct {
	db  *sql.DB
	cfg Config
}

// Open opens (creating if needed) the database and applies migrations.
func Open(ctx context.Context, path string, cfg Config) (*Store, error) {
	if strings.TrimSpace(path) == "" {
		return nil, fmt.Errorf("%w: database path is required", ErrInvalid)
	}
	dsn := path
	if path == ":memory:" {
		dsn = "file::memory:"
	}
	sep := "?"
	if strings.Contains(dsn, "?") {
		sep = "&"
	}
	dsn += sep + "_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)&_pragma=synchronous(NORMAL)&_pragma=foreign_keys(ON)"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("open sqlite: %w", err)
	}
	// One connection serializes all writes and avoids SQLITE_BUSY between the
	// tick loop and request handlers.
	db.SetMaxOpenConns(1)
	db.SetMaxIdleConns(1)
	db.SetConnMaxLifetime(0)
	if err := db.PingContext(ctx); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("ping sqlite: %w", err)
	}
	s := &Store{db: db, cfg: cfg.normalized()}
	if err := s.migrate(ctx); err != nil {
		_ = db.Close()
		return nil, err
	}
	return s, nil
}

// Close closes the database.
func (s *Store) Close() error { return s.db.Close() }

// Config returns the normalized configuration.
func (s *Store) Config() Config { return s.cfg }

func (s *Store) migrate(ctx context.Context) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin migration: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	if _, err := tx.ExecContext(ctx, `CREATE TABLE IF NOT EXISTS schema_meta (version INTEGER NOT NULL)`); err != nil {
		return fmt.Errorf("create schema_meta: %w", err)
	}
	var version int
	err = tx.QueryRowContext(ctx, `SELECT version FROM schema_meta LIMIT 1`).Scan(&version)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		if _, err := tx.ExecContext(ctx, `INSERT INTO schema_meta(version) VALUES (0)`); err != nil {
			return fmt.Errorf("seed schema_meta: %w", err)
		}
		version = 0
	case err != nil:
		return fmt.Errorf("read schema version: %w", err)
	}
	if version > SchemaVersion {
		return fmt.Errorf("database schema version %d is newer than supported %d", version, SchemaVersion)
	}

	if version < 1 {
		if err := applyV1(ctx, tx); err != nil {
			return err
		}
		version = 1
	}
	if _, err := tx.ExecContext(ctx, `UPDATE schema_meta SET version = ?`, version); err != nil {
		return fmt.Errorf("write schema version: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit migration: %w", err)
	}
	return nil
}

func applyV1(ctx context.Context, tx *sql.Tx) error {
	stmts := []string{
		`CREATE TABLE IF NOT EXISTS schedules (
			id TEXT PRIMARY KEY,
			kind INTEGER NOT NULL,
			label TEXT NOT NULL,
			timezone TEXT NOT NULL,
			at_hour INTEGER NOT NULL,
			at_minute INTEGER NOT NULL,
			at_second INTEGER NOT NULL,
			once_date TEXT NOT NULL,
			repeat_kind INTEGER NOT NULL,
			weekdays TEXT NOT NULL,
			next_due_unix INTEGER NOT NULL,
			enabled INTEGER NOT NULL,
			payload_json TEXT NOT NULL,
			revision INTEGER NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL,
			deleted_unix INTEGER NOT NULL DEFAULT 0,
			last_fired_unix INTEGER NOT NULL DEFAULT 0,
			snooze_until_unix INTEGER NOT NULL DEFAULT 0,
			snooze_count INTEGER NOT NULL DEFAULT 0,
			active_occurrence_unix INTEGER NOT NULL DEFAULT 0,
			active_until_unix INTEGER NOT NULL DEFAULT 0
		)`,
		`CREATE INDEX IF NOT EXISTS schedules_due ON schedules(enabled, next_due_unix)`,
		`CREATE TABLE IF NOT EXISTS timers (
			id TEXT PRIMARY KEY,
			label TEXT NOT NULL,
			duration_seconds INTEGER NOT NULL,
			remaining_seconds INTEGER NOT NULL,
			due_unix INTEGER NOT NULL,
			state INTEGER NOT NULL,
			revision INTEGER NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL
		)`,
		`CREATE INDEX IF NOT EXISTS timers_due ON timers(state, due_unix)`,
		`CREATE TABLE IF NOT EXISTS events (
			id TEXT PRIMARY KEY,
			dedupe_key TEXT NOT NULL UNIQUE,
			source_id TEXT NOT NULL,
			source_kind INTEGER NOT NULL,
			action INTEGER NOT NULL DEFAULT 1,
			due_unix INTEGER NOT NULL,
			occurrence_unix INTEGER NOT NULL,
			label TEXT NOT NULL,
			payload_json TEXT NOT NULL,
			state INTEGER NOT NULL,
			lease_owner TEXT NOT NULL DEFAULT '',
			lease_until_unix INTEGER NOT NULL DEFAULT 0,
			attempts INTEGER NOT NULL DEFAULT 0,
			snooze_seq INTEGER NOT NULL DEFAULT 0,
			detail TEXT NOT NULL DEFAULT '',
			created_unix INTEGER NOT NULL,
			acked_unix INTEGER NOT NULL DEFAULT 0,
			ack_outcome INTEGER NOT NULL DEFAULT 0,
			task_type TEXT NOT NULL DEFAULT '',
			queue TEXT NOT NULL DEFAULT ''
		)`,
		`CREATE INDEX IF NOT EXISTS events_pending ON events(state, due_unix)`,
		`CREATE INDEX IF NOT EXISTS events_source ON events(source_id, state)`,
		`CREATE INDEX IF NOT EXISTS events_lease ON events(state, lease_until_unix)`,
		`CREATE INDEX IF NOT EXISTS events_queue ON events(queue, state, due_unix)`,
		`CREATE TABLE IF NOT EXISTS idempotency (
			key TEXT PRIMARY KEY,
			resource_kind TEXT NOT NULL,
			resource_id TEXT NOT NULL,
			request_hash TEXT NOT NULL,
			created_unix INTEGER NOT NULL
		)`,
		`CREATE TABLE IF NOT EXISTS consumers (
			consumer_id TEXT PRIMARY KEY,
			last_seen_unix INTEGER NOT NULL,
			last_ack_unix INTEGER NOT NULL DEFAULT 0
		)`,
	}
	for _, stmt := range stmts {
		if _, err := tx.ExecContext(ctx, stmt); err != nil {
			return fmt.Errorf("apply schema: %w", err)
		}
	}
	return nil
}

func (s *Store) begin(ctx context.Context) (*sql.Tx, error) {
	return s.db.BeginTx(ctx, nil)
}

func weekdaysToText(days []int32) string {
	if len(days) == 0 {
		return ""
	}
	sorted := make([]int32, len(days))
	copy(sorted, days)
	for i := 1; i < len(sorted); i++ {
		for j := i; j > 0 && sorted[j] < sorted[j-1]; j-- {
			sorted[j], sorted[j-1] = sorted[j-1], sorted[j]
		}
	}
	parts := make([]string, 0, len(sorted))
	for _, d := range sorted {
		parts = append(parts, strconv.Itoa(int(d)))
	}
	return strings.Join(parts, ",")
}

func weekdaysFromText(s string) []int32 {
	if s == "" {
		return nil
	}
	parts := strings.Split(s, ",")
	out := make([]int32, 0, len(parts))
	for _, p := range parts {
		n, err := strconv.Atoi(strings.TrimSpace(p))
		if err != nil {
			continue
		}
		out = append(out, int32(n))
	}
	return out
}

func boolToInt(v bool) int {
	if v {
		return 1
	}
	return 0
}

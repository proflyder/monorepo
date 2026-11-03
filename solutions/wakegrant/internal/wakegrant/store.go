package wakegrant

import (
	"context"
	"crypto/rand"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

// SchemaVersion is the current SQLite schema version.
const SchemaVersion = 1

// Store is the durable grant store. It lives on a PVC so a pod restart between
// the Wake-on-LAN packet and the machine's claim does not lose the grant.
type Store struct {
	db *sql.DB
}

// OpenStore opens (creating if needed) the database and applies migrations.
func OpenStore(ctx context.Context, path string) (*Store, error) {
	if strings.TrimSpace(path) == "" {
		return nil, fmt.Errorf("database path is required")
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
	// One connection serializes every writer, so two concurrent claims cannot
	// observe the same unconsumed row.
	db.SetMaxOpenConns(1)
	db.SetMaxIdleConns(1)
	db.SetConnMaxLifetime(0)
	if err := db.PingContext(ctx); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("ping sqlite: %w", err)
	}
	s := &Store{db: db}
	if err := s.migrate(ctx); err != nil {
		_ = db.Close()
		return nil, err
	}
	return s, nil
}

// Close closes the database.
func (s *Store) Close() error { return s.db.Close() }

// Ping checks the database connection (used by /healthz).
func (s *Store) Ping(ctx context.Context) error { return s.db.PingContext(ctx) }

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
		if _, err := tx.ExecContext(ctx, `CREATE TABLE IF NOT EXISTS grants (
			id TEXT PRIMARY KEY,
			machine TEXT NOT NULL,
			created_unix INTEGER NOT NULL,
			expires_unix INTEGER NOT NULL,
			consumed INTEGER NOT NULL DEFAULT 0,
			consumed_unix INTEGER NOT NULL DEFAULT 0
		)`); err != nil {
			return fmt.Errorf("create grants: %w", err)
		}
		if _, err := tx.ExecContext(ctx, `CREATE INDEX IF NOT EXISTS grants_machine ON grants(machine, consumed, expires_unix)`); err != nil {
			return fmt.Errorf("create grants index: %w", err)
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

// CreateGrant stores a new grant and replaces any unconsumed grant for the
// same machine, so only the newest one can ever be claimed.
func (s *Store) CreateGrant(ctx context.Context, machine string, now time.Time, ttl time.Duration) (string, time.Time, error) {
	id, err := newUUID()
	if err != nil {
		return "", time.Time{}, err
	}
	expires := now.Add(ttl)
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return "", time.Time{}, fmt.Errorf("begin grant: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := tx.ExecContext(ctx, `DELETE FROM grants WHERE machine = ? AND consumed = 0`, machine); err != nil {
		return "", time.Time{}, fmt.Errorf("replace grant: %w", err)
	}
	if _, err := tx.ExecContext(ctx,
		`INSERT INTO grants(id, machine, created_unix, expires_unix, consumed) VALUES (?, ?, ?, ?, 0)`,
		id, machine, now.UnixNano(), expires.Unix(),
	); err != nil {
		return "", time.Time{}, fmt.Errorf("insert grant: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return "", time.Time{}, fmt.Errorf("commit grant: %w", err)
	}
	return id, expires.UTC(), nil
}

// ConsumeNewest atomically consumes the newest unexpired unconsumed grant for
// machine. It reports false when there is nothing to grant.
func (s *Store) ConsumeNewest(ctx context.Context, machine string, now time.Time) (string, bool, error) {
	var id string
	err := s.db.QueryRowContext(ctx,
		`UPDATE grants SET consumed = 1, consumed_unix = ?
		 WHERE rowid = (
			SELECT rowid FROM grants
			WHERE machine = ? AND consumed = 0 AND expires_unix > ?
			ORDER BY rowid DESC LIMIT 1
		 )
		 RETURNING id`,
		now.UnixNano(), machine, now.Unix(),
	).Scan(&id)
	if errors.Is(err, sql.ErrNoRows) {
		return "", false, nil
	}
	if err != nil {
		return "", false, fmt.Errorf("consume grant: %w", err)
	}
	return id, true, nil
}

// newUUID returns a random RFC 4122 version 4 UUID.
func newUUID() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", fmt.Errorf("generate id: %w", err)
	}
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16]), nil
}

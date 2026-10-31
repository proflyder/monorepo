// Package store persists currency history and sent-alert deduplication state
// in SQLite (modernc.org/sqlite, WAL). It is the migration target for the two
// former H2 tables.
package store

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	_ "modernc.org/sqlite"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

// Schema:
//
//	currency_history(id, timestamp, usd_buy, usd_sell, rub_buy, rub_sell)
//	  timestamp is Unix milliseconds (UTC) so ordering is exact.
//	sent_alerts(id, pair, period, rate_type, level, direction,
//	            rate_at_alert, change_percent)
//
// The sent_alerts unique index mirrors the H2 unique key (pair, period,
// rate_type); recording an alert is delete-then-insert.
const schema = `
CREATE TABLE IF NOT EXISTS currency_history (
	id        INTEGER PRIMARY KEY AUTOINCREMENT,
	timestamp INTEGER NOT NULL,
	usd_buy   REAL NOT NULL,
	usd_sell  REAL NOT NULL,
	rub_buy   REAL NOT NULL,
	rub_sell  REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS currency_history_timestamp ON currency_history(timestamp);

CREATE TABLE IF NOT EXISTS sent_alerts (
	id             INTEGER PRIMARY KEY AUTOINCREMENT,
	pair           TEXT NOT NULL,
	period         TEXT NOT NULL,
	rate_type      TEXT NOT NULL,
	level          TEXT NOT NULL,
	direction      TEXT NOT NULL,
	rate_at_alert  REAL NOT NULL,
	change_percent REAL NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS sent_alerts_key ON sent_alerts(pair, period, rate_type);
`

// ErrNotFound is returned when a required row does not exist.
var ErrNotFound = errors.New("not found")

// Store is a SQLite-backed currency store.
type Store struct {
	db *sql.DB
}

// Open creates the parent directory, opens the database in WAL mode and
// applies the schema.
func Open(ctx context.Context, path string) (*Store, error) {
	if path == "" {
		return nil, errors.New("database path is required")
	}
	if path != ":memory:" && !isMemoryDSN(path) {
		if dir := filepath.Dir(path); dir != "" && dir != "." {
			if err := os.MkdirAll(dir, 0o750); err != nil {
				return nil, fmt.Errorf("create database directory: %w", err)
			}
		}
	}
	dsn := path
	if !isMemoryDSN(path) {
		dsn = "file:" + path + "?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)&_pragma=foreign_keys(1)"
	}
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, err
	}
	// Exactly one connection serializes writes, matching the scheduler's
	// approach and avoiding SQLITE_BUSY under WAL.
	db.SetMaxOpenConns(1)
	if err := db.PingContext(ctx); err != nil {
		_ = db.Close()
		return nil, err
	}
	if _, err := db.ExecContext(ctx, schema); err != nil {
		_ = db.Close()
		return nil, err
	}
	return &Store{db: db}, nil
}

func isMemoryDSN(path string) bool {
	return path == ":memory:" || strings.HasPrefix(path, "file::memory:") || strings.HasPrefix(path, "file:memdb")
}

// Close closes the database.
func (s *Store) Close() error { return s.db.Close() }

// Ping verifies the database is usable.
func (s *Store) Ping(ctx context.Context) error { return s.db.PingContext(ctx) }

// SaveRecord inserts one history row and returns the total record count.
func (s *Store) SaveRecord(ctx context.Context, record model.Record) (int, error) {
	ts := record.Timestamp.UTC().Truncate(time.Millisecond).UnixMilli()
	if _, err := s.db.ExecContext(ctx,
		`INSERT INTO currency_history(timestamp, usd_buy, usd_sell, rub_buy, rub_sell) VALUES(?,?,?,?,?)`,
		ts, record.Rates.USDToKZT.Buy, record.Rates.USDToKZT.Sell, record.Rates.RUBToKZT.Buy, record.Rates.RUBToKZT.Sell,
	); err != nil {
		return 0, err
	}
	return s.CountRecords(ctx)
}

// CountRecords returns the number of history rows.
func (s *Store) CountRecords(ctx context.Context) (int, error) {
	var n int
	err := s.db.QueryRowContext(ctx, `SELECT COUNT(*) FROM currency_history`).Scan(&n)
	return n, err
}

const recordColumns = `timestamp, usd_buy, usd_sell, rub_buy, rub_sell`

func scanRecord(scan func(dest ...any) error) (model.Record, error) {
	var ts int64
	var usdBuy, usdSell, rubBuy, rubSell float64
	if err := scan(&ts, &usdBuy, &usdSell, &rubBuy, &rubSell); err != nil {
		return model.Record{}, err
	}
	return model.Record{
		Timestamp: time.UnixMilli(ts).UTC(),
		Rates: model.Rates{
			USDToKZT: model.ExchangeRate{Buy: usdBuy, Sell: usdSell},
			RUBToKZT: model.ExchangeRate{Buy: rubBuy, Sell: rubSell},
		},
	}, nil
}

// AllRecords returns the full history, newest first.
func (s *Store) AllRecords(ctx context.Context) ([]model.Record, error) {
	rows, err := s.db.QueryContext(ctx, `SELECT `+recordColumns+` FROM currency_history ORDER BY timestamp DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return collectRecords(rows)
}

// RecordsByDateRange returns records with from <= timestamp <= to, newest first.
func (s *Store) RecordsByDateRange(ctx context.Context, from, to time.Time) ([]model.Record, error) {
	rows, err := s.db.QueryContext(ctx,
		`SELECT `+recordColumns+` FROM currency_history WHERE timestamp >= ? AND timestamp <= ? ORDER BY timestamp DESC`,
		from.UTC().UnixMilli(), to.UTC().UnixMilli())
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return collectRecords(rows)
}

func collectRecords(rows *sql.Rows) ([]model.Record, error) {
	records := make([]model.Record, 0)
	for rows.Next() {
		rec, err := scanRecord(rows.Scan)
		if err != nil {
			return nil, err
		}
		records = append(records, rec)
	}
	return records, rows.Err()
}

// LatestRecord returns the newest row, or ErrNotFound on an empty table.
func (s *Store) LatestRecord(ctx context.Context) (model.Record, error) {
	row := s.db.QueryRowContext(ctx, `SELECT `+recordColumns+` FROM currency_history ORDER BY timestamp DESC LIMIT 1`)
	rec, err := scanRecord(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return model.Record{}, ErrNotFound
	}
	return rec, err
}

// RecordBefore returns the newest record at or before the cutoff. This is the
// Kotlin "newest record that is at least N old" semantics, not closest-to.
func (s *Store) RecordBefore(ctx context.Context, cutoff time.Time) (model.Record, error) {
	row := s.db.QueryRowContext(ctx,
		`SELECT `+recordColumns+` FROM currency_history WHERE timestamp <= ? ORDER BY timestamp DESC LIMIT 1`,
		cutoff.UTC().UnixMilli())
	rec, err := scanRecord(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return model.Record{}, ErrNotFound
	}
	return rec, err
}

// DeleteAll removes every history row and returns the deleted count. The
// sent_alerts table is deliberately untouched, matching the Kotlin API.
func (s *Store) DeleteAll(ctx context.Context) (int, error) {
	res, err := s.db.ExecContext(ctx, `DELETE FROM currency_history`)
	if err != nil {
		return 0, err
	}
	n, err := res.RowsAffected()
	return int(n), err
}

// CleanOldRecords deletes rows with timestamp <= now-olderThan.
func (s *Store) CleanOldRecords(ctx context.Context, olderThan time.Duration, now time.Time) (int, error) {
	cutoff := now.Add(-olderThan).UTC().UnixMilli()
	res, err := s.db.ExecContext(ctx, `DELETE FROM currency_history WHERE timestamp <= ?`, cutoff)
	if err != nil {
		return 0, err
	}
	n, err := res.RowsAffected()
	return int(n), err
}

// GetSentAlert returns the last alert for a key, or ErrNotFound.
func (s *Store) GetSentAlert(ctx context.Context, key model.AlertKey) (model.SentAlert, error) {
	var out model.SentAlert
	err := s.db.QueryRowContext(ctx,
		`SELECT pair, period, rate_type, level, direction, rate_at_alert, change_percent
		 FROM sent_alerts WHERE pair = ? AND period = ? AND rate_type = ?`,
		string(key.Pair), key.Period, string(key.RateType),
	).Scan(&out.Key.Pair, &out.Key.Period, &out.Key.RateType, &out.Level, &out.Direction, &out.RateAtAlert, &out.ChangePercent)
	if errors.Is(err, sql.ErrNoRows) {
		return model.SentAlert{}, ErrNotFound
	}
	return out, err
}

// RecordSentAlert upserts one sent alert (delete-then-insert).
func (s *Store) RecordSentAlert(ctx context.Context, alert model.SentAlert) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := tx.ExecContext(ctx,
		`DELETE FROM sent_alerts WHERE pair = ? AND period = ? AND rate_type = ?`,
		string(alert.Key.Pair), alert.Key.Period, string(alert.Key.RateType),
	); err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx,
		`INSERT INTO sent_alerts(pair, period, rate_type, level, direction, rate_at_alert, change_percent)
		 VALUES(?,?,?,?,?,?,?)`,
		string(alert.Key.Pair), alert.Key.Period, string(alert.Key.RateType), string(alert.Level),
		string(alert.Direction), alert.RateAtAlert, alert.ChangePercent,
	); err != nil {
		return err
	}
	return tx.Commit()
}

// ClearSentAlert deletes the dedup row for a key; a missing key is not an error.
func (s *Store) ClearSentAlert(ctx context.Context, key model.AlertKey) error {
	_, err := s.db.ExecContext(ctx,
		`DELETE FROM sent_alerts WHERE pair = ? AND period = ? AND rate_type = ?`,
		string(key.Pair), key.Period, string(key.RateType))
	return err
}

// CountSentAlerts returns the number of dedup rows (used by tests/import).
func (s *Store) CountSentAlerts(ctx context.Context) (int, error) {
	var n int
	err := s.db.QueryRowContext(ctx, `SELECT COUNT(*) FROM sent_alerts`).Scan(&n)
	return n, err
}

// InsertRecordIfAbsent inserts a record only when no row has the same
// timestamp; it is the idempotent primitive used by the import tool.
func (s *Store) InsertRecordIfAbsent(ctx context.Context, record model.Record) (bool, error) {
	ts := record.Timestamp.UTC().Truncate(time.Millisecond).UnixMilli()
	res, err := s.db.ExecContext(ctx,
		`INSERT INTO currency_history(timestamp, usd_buy, usd_sell, rub_buy, rub_sell)
		 SELECT ?,?,?,?,? WHERE NOT EXISTS (SELECT 1 FROM currency_history WHERE timestamp = ?)`,
		ts, record.Rates.USDToKZT.Buy, record.Rates.USDToKZT.Sell, record.Rates.RUBToKZT.Buy, record.Rates.RUBToKZT.Sell, ts)
	if err != nil {
		return false, err
	}
	n, err := res.RowsAffected()
	return n > 0, err
}

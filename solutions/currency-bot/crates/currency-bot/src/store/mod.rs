//! SQLite persistence on the shared single-writer [`home_sqlite::Db`].
//!
//! The DDL is the Go store's, statement for statement. `home-sqlite` adds
//! `schema_meta(version)`; the Go binary ignores that table, so a PVC created
//! by either implementation opens in the other without a migration.

use std::fmt;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use home_sqlite::{Db, Migration, SqliteError};
use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension as _, params};

use crate::domain::model::{
    AlertKey, Direction, ExchangeRate, Level, Pair, RateType, Rates, Record, SentAlert,
};

/// Schema version written to `schema_meta`.
pub const SCHEMA_VERSION: i64 = 1;

/// Exact DDL from the Go store (`internal/store/store.go`).
const V1: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS currency_history (
			id        INTEGER PRIMARY KEY AUTOINCREMENT,
			timestamp INTEGER NOT NULL,
			usd_buy   REAL NOT NULL,
			usd_sell  REAL NOT NULL,
			rub_buy   REAL NOT NULL,
			rub_sell  REAL NOT NULL
		)",
    "CREATE INDEX IF NOT EXISTS currency_history_timestamp ON currency_history(timestamp)",
    "CREATE TABLE IF NOT EXISTS sent_alerts (
			id             INTEGER PRIMARY KEY AUTOINCREMENT,
			pair           TEXT NOT NULL,
			period         TEXT NOT NULL,
			rate_type      TEXT NOT NULL,
			level          TEXT NOT NULL,
			direction      TEXT NOT NULL,
			rate_at_alert  REAL NOT NULL,
			change_percent REAL NOT NULL
		)",
    "CREATE UNIQUE INDEX IF NOT EXISTS sent_alerts_key ON sent_alerts(pair, period, rate_type)",
];

/// One ordered schema step.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: SCHEMA_VERSION,
    description: "currency-bot v1",
    statements: V1,
}];

const RECORD_COLUMNS: &str = "timestamp, usd_buy, usd_sell, rub_buy, rub_sell";

/// A store failure.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Sqlite(#[from] SqliteError),
}

/// SQLite-backed currency store.
pub struct Store {
    db: Mutex<Db>,
}

impl fmt::Debug for Store {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Store {
    /// Open (creating if needed) the database, apply the pragmas and the v1
    /// schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let db = Db::open(path, MIGRATIONS)?;
        Ok(Self { db: Mutex::new(db) })
    }

    /// In-memory database with the same schema, for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let db = Db::open_in_memory(MIGRATIONS)?;
        Ok(Self { db: Mutex::new(db) })
    }

    fn call<T>(
        &self,
        query: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, StoreError>
    where
        T: Send + 'static,
    {
        let guard = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        guard.call_blocking(query).map_err(StoreError::from)
    }

    /// Verify the database is usable.
    pub fn ping(&self) -> Result<(), StoreError> {
        self.call(|connection| connection.query_row("SELECT 1", [], |_| Ok(())))
    }

    /// Stop the writer thread and close the connection.
    pub fn shutdown(self) -> Result<(), StoreError> {
        let db = self.db.into_inner().unwrap_or_else(PoisonError::into_inner);
        db.shutdown().map_err(StoreError::from)
    }

    /// Insert one history row and return the total record count.
    pub fn save_record(&self, record: &Record) -> Result<usize, StoreError> {
        let timestamp = record.timestamp.as_millisecond();
        let usd_buy = record.rates.usd_to_kzt.buy;
        let usd_sell = record.rates.usd_to_kzt.sell;
        let rub_buy = record.rates.rub_to_kzt.buy;
        let rub_sell = record.rates.rub_to_kzt.sell;
        self.call(move |connection| {
            connection.execute(
                "INSERT INTO currency_history(timestamp, usd_buy, usd_sell, rub_buy, rub_sell) VALUES(?,?,?,?,?)",
                params![timestamp, usd_buy, usd_sell, rub_buy, rub_sell],
            )?;
            count(connection, "currency_history")
        })
    }

    /// Number of history rows.
    pub fn count_records(&self) -> Result<usize, StoreError> {
        self.call(|connection| count(connection, "currency_history"))
    }

    /// Full history, newest first.
    pub fn all_records(&self) -> Result<Vec<Record>, StoreError> {
        self.call(|connection| {
            let sql =
                format!("SELECT {RECORD_COLUMNS} FROM currency_history ORDER BY timestamp DESC");
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map([], scan_record)?;
            rows.collect()
        })
    }

    /// Records with `from <= timestamp <= to`, newest first.
    pub fn records_by_date_range(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<Record>, StoreError> {
        let from = from.as_millisecond();
        let to = to.as_millisecond();
        self.call(move |connection| {
            let sql = format!(
                "SELECT {RECORD_COLUMNS} FROM currency_history WHERE timestamp >= ? AND timestamp <= ? ORDER BY timestamp DESC"
            );
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params![from, to], scan_record)?;
            rows.collect()
        })
    }

    /// Newest row, or [`StoreError::NotFound`] on an empty table.
    pub fn latest_record(&self) -> Result<Record, StoreError> {
        self.call(|connection| {
            let sql = format!(
                "SELECT {RECORD_COLUMNS} FROM currency_history ORDER BY timestamp DESC LIMIT 1"
            );
            let record = connection.query_row(&sql, [], scan_record).optional()?;
            record.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
        })
        .map_err(map_not_found)
    }

    /// Newest record at or before `cutoff`; the Kotlin "at least N old"
    /// semantics, not closest-to.
    pub fn record_before(&self, cutoff: Timestamp) -> Result<Record, StoreError> {
        let cutoff = cutoff.as_millisecond();
        self.call(move |connection| {
            let sql = format!(
                "SELECT {RECORD_COLUMNS} FROM currency_history WHERE timestamp <= ? ORDER BY timestamp DESC LIMIT 1"
            );
            let record = connection
                .query_row(&sql, params![cutoff], scan_record)
                .optional()?;
            record.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
        })
        .map_err(map_not_found)
    }

    /// Delete every history row and return the deleted count. `sent_alerts` is
    /// deliberately untouched, matching the Kotlin API.
    pub fn delete_all(&self) -> Result<usize, StoreError> {
        self.call(|connection| connection.execute("DELETE FROM currency_history", []))
    }

    /// Delete rows with `timestamp <= now - retention`.
    pub fn clean_old_records(
        &self,
        retention: std::time::Duration,
        now: Timestamp,
    ) -> Result<usize, StoreError> {
        let retention = i64::try_from(retention.as_millis()).unwrap_or(i64::MAX);
        let cutoff = now.as_millisecond().saturating_sub(retention);
        self.call(move |connection| {
            connection.execute(
                "DELETE FROM currency_history WHERE timestamp <= ?",
                params![cutoff],
            )
        })
    }

    /// The last alert for a key, or `None` when absent (or its stored
    /// enumerations are not recognised, which Go treated as "no previous").
    pub fn get_sent_alert(&self, key: &AlertKey) -> Result<Option<SentAlert>, StoreError> {
        let pair = key.pair.as_str();
        let rate_type = key.rate_type.as_str();
        let period = key.period.clone();
        self.call(move |connection| {
            let row = connection
                .query_row(
                    "SELECT pair, period, rate_type, level, direction, rate_at_alert, change_percent
                     FROM sent_alerts WHERE pair = ? AND period = ? AND rate_type = ?",
                    params![pair, period, rate_type],
                    |row| {
                        Ok(StoredAlert {
                            pair: row.get::<_, String>(0)?,
                            period: row.get::<_, String>(1)?,
                            rate_type: row.get::<_, String>(2)?,
                            level: row.get::<_, String>(3)?,
                            direction: row.get::<_, String>(4)?,
                            rate_at_alert: row.get::<_, f64>(5)?,
                            change_percent: row.get::<_, f64>(6)?,
                        })
                    },
                )
                .optional()?;
            Ok(row.and_then(StoredAlert::into_sent))
        })
    }

    /// Upsert one sent alert (delete-then-insert), mirroring the H2 unique key.
    pub fn record_sent_alert(&self, alert: &SentAlert) -> Result<(), StoreError> {
        let pair = alert.key.pair.as_str().to_owned();
        let period = alert.key.period.clone();
        let rate_type = alert.key.rate_type.as_str().to_owned();
        let level = alert.level.as_str().to_owned();
        let direction = alert.direction.as_str().to_owned();
        let rate_at_alert = alert.rate_at_alert;
        let change_percent = alert.change_percent;
        self.call(move |connection| {
            let transaction = connection.transaction()?;
            transaction.execute(
                "DELETE FROM sent_alerts WHERE pair = ? AND period = ? AND rate_type = ?",
                params![pair, period, rate_type],
            )?;
            transaction.execute(
                "INSERT INTO sent_alerts(pair, period, rate_type, level, direction, rate_at_alert, change_percent)
                 VALUES(?,?,?,?,?,?,?)",
                params![pair, period, rate_type, level, direction, rate_at_alert, change_percent],
            )?;
            transaction.commit()
        })
    }

    /// Delete the dedup row for a key; a missing key is not an error.
    pub fn clear_sent_alert(&self, key: &AlertKey) -> Result<(), StoreError> {
        let pair = key.pair.as_str().to_owned();
        let period = key.period.clone();
        let rate_type = key.rate_type.as_str().to_owned();
        self.call(move |connection| {
            connection.execute(
                "DELETE FROM sent_alerts WHERE pair = ? AND period = ? AND rate_type = ?",
                params![pair, period, rate_type],
            )?;
            Ok(())
        })
    }

    /// Number of dedup rows (tests and diagnostics).
    pub fn count_sent_alerts(&self) -> Result<usize, StoreError> {
        self.call(|connection| count(connection, "sent_alerts"))
    }
}

fn map_not_found(error: StoreError) -> StoreError {
    match error {
        StoreError::Sqlite(SqliteError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
            StoreError::NotFound
        }
        other => other,
    }
}

fn count(connection: &Connection, table: &str) -> rusqlite::Result<usize> {
    let value: i64 = connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })?;
    Ok(usize::try_from(value).unwrap_or(0))
}

fn scan_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    let timestamp: i64 = row.get(0)?;
    let usd_buy: f64 = row.get(1)?;
    let usd_sell: f64 = row.get(2)?;
    let rub_buy: f64 = row.get(3)?;
    let rub_sell: f64 = row.get(4)?;
    Ok(Record {
        timestamp: Timestamp::from_millisecond(timestamp).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        rates: Rates {
            usd_to_kzt: ExchangeRate {
                buy: usd_buy,
                sell: usd_sell,
            },
            rub_to_kzt: ExchangeRate {
                buy: rub_buy,
                sell: rub_sell,
            },
        },
    })
}

struct StoredAlert {
    pair: String,
    period: String,
    rate_type: String,
    level: String,
    direction: String,
    rate_at_alert: f64,
    change_percent: f64,
}

impl StoredAlert {
    fn into_sent(self) -> Option<SentAlert> {
        Some(SentAlert {
            key: AlertKey::new(
                Pair::parse(&self.pair)?,
                self.period,
                RateType::parse(&self.rate_type)?,
            ),
            level: Level::parse(&self.level)?,
            direction: Direction::parse(&self.direction)?,
            rate_at_alert: self.rate_at_alert,
            change_percent: self.change_percent,
            sent_at: None,
        })
    }
}

#[cfg(test)]
mod tests;

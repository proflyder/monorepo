//! Durable scheduler store on `home-sqlite`.
//!
//! The schema, SQL conditions, error texts and event identities are the former
//! Go store's, byte for byte. A database created by the Go service opens here
//! and is migrated in place: [`MIGRATIONS`] carries the identical v1 DDL and
//! then adds the v2 scenario table. An older binary rejects a newer database.
//!
//! Every mutation runs on the single SQLite writer thread; `now` is always an
//! argument so tick scenarios stay deterministic.

mod event;
mod push;
mod scenario;
mod schedule;
mod status;
mod task;
mod tick;
mod timer;

use std::fmt;
use std::sync::{Mutex, PoisonError};

use home_sqlite::{Db, Migration, SqliteError};
use rusqlite::{Connection, Transaction};

use crate::domain::{DomainError, ErrorKind};

pub use crate::domain::{
    AckOutcome, Event, EventAction, EventState, Scenario, ScenarioCondition, ScenarioDefinition,
    ScenarioStep, ScenarioTimeTrigger, ScenarioTrigger, Schedule, ScheduleKind, SourceKind, Timer,
    TimerState,
};
pub use push::{PushDevice, PushDeviceInput};
pub use status::{ConsumerStatus, QueueInfo, Status};
pub use tick::TickStats;

use crate::domain::ScheduleKind as DomainScheduleKind;

/// Schema version written to `schema_meta`. v2 adds the scenario table and v3
/// the push device registry; an older database migrates in place, and an older
/// binary rejects a newer one.
pub const SCHEMA_VERSION: i64 = 3;

/// Queue names used by the built-in domains.
pub const QUEUE_HOME: &str = "home";
pub const QUEUE_TASKS: &str = "tasks";
pub const QUEUE_SCENARIO: &str = "scenario";

/// DDL copied from the Go `applyV1` statements in the same order.
const V1: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS schedules (
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
		)",
    "CREATE INDEX IF NOT EXISTS schedules_due ON schedules(enabled, next_due_unix)",
    "CREATE TABLE IF NOT EXISTS timers (
			id TEXT PRIMARY KEY,
			label TEXT NOT NULL,
			duration_seconds INTEGER NOT NULL,
			remaining_seconds INTEGER NOT NULL,
			due_unix INTEGER NOT NULL,
			state INTEGER NOT NULL,
			revision INTEGER NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL
		)",
    "CREATE INDEX IF NOT EXISTS timers_due ON timers(state, due_unix)",
    "CREATE TABLE IF NOT EXISTS events (
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
		)",
    "CREATE INDEX IF NOT EXISTS events_pending ON events(state, due_unix)",
    "CREATE INDEX IF NOT EXISTS events_source ON events(source_id, state)",
    "CREATE INDEX IF NOT EXISTS events_lease ON events(state, lease_until_unix)",
    "CREATE INDEX IF NOT EXISTS events_queue ON events(queue, state, due_unix)",
    "CREATE TABLE IF NOT EXISTS idempotency (
			key TEXT PRIMARY KEY,
			resource_kind TEXT NOT NULL,
			resource_id TEXT NOT NULL,
			request_hash TEXT NOT NULL,
			created_unix INTEGER NOT NULL
		)",
    "CREATE TABLE IF NOT EXISTS consumers (
			consumer_id TEXT PRIMARY KEY,
			last_seen_unix INTEGER NOT NULL,
			last_ack_unix INTEGER NOT NULL DEFAULT 0
		)",
];

/// v2 adds durable scenario definitions. Time triggers are materialized as
/// `schedules` rows (kind=SCENARIO), so recurrence, DST, dedupe and the outbox
/// stay shared with alarms and reminders.
const V2: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS scenarios (
			id TEXT PRIMARY KEY,
			body_json TEXT NOT NULL,
			enabled INTEGER NOT NULL,
			revision INTEGER NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL
		)",
    "CREATE INDEX IF NOT EXISTS scenarios_enabled ON scenarios(enabled)",
];

/// v3 adds the push device registry: FCM tokens of the owner's phones. The
/// scheduler stores them only; `solutions/phone-push` sends.
const V3: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS push_devices (
			token TEXT PRIMARY KEY,
			platform TEXT NOT NULL,
			app_version TEXT NOT NULL,
			device_name TEXT NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL
		)",
    "CREATE INDEX IF NOT EXISTS push_devices_updated ON push_devices(updated_unix)",
];

/// One ordered schema step.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "scheduler v1",
        statements: V1,
    },
    Migration {
        version: 2,
        description: "scheduler v2 scenarios",
        statements: V2,
    },
    Migration {
        version: SCHEMA_VERSION,
        description: "scheduler v3 push devices",
        statements: V3,
    },
];

/// Operational bounds; zero values are replaced by [`Config::default`].
#[derive(Debug, Clone)]
pub struct Config {
    pub default_timezone: String,
    pub missed_grace: i64,
    pub ring_window: i64,
    pub max_timers: i64,
    pub max_timer_duration: i64,
    pub max_payload_bytes: usize,
    pub max_attempts: i64,
    pub default_snooze_minutes: i32,
    pub event_retention: i64,
    pub consumer_liveness: i64,
    pub max_lease: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_timezone: "UTC".to_owned(),
            missed_grace: 600,
            ring_window: 900,
            max_timers: 64,
            max_timer_duration: 7 * 24 * 60 * 60,
            max_payload_bytes: 16 * 1024,
            max_attempts: 5,
            default_snooze_minutes: 9,
            event_retention: 30 * 24 * 60 * 60,
            consumer_liveness: 90,
            max_lease: 60 * 60,
        }
    }
}

impl Config {
    fn normalized(self) -> Self {
        let default = Self::default();
        Self {
            default_timezone: if self.default_timezone.is_empty() {
                default.default_timezone
            } else {
                self.default_timezone
            },
            missed_grace: if self.missed_grace <= 0 {
                default.missed_grace
            } else {
                self.missed_grace
            },
            ring_window: if self.ring_window <= 0 {
                default.ring_window
            } else {
                self.ring_window
            },
            max_timers: if self.max_timers <= 0 {
                default.max_timers
            } else {
                self.max_timers
            },
            max_timer_duration: if self.max_timer_duration <= 0 {
                default.max_timer_duration
            } else {
                self.max_timer_duration
            },
            max_payload_bytes: if self.max_payload_bytes == 0 {
                default.max_payload_bytes
            } else {
                self.max_payload_bytes
            },
            max_attempts: if self.max_attempts <= 0 {
                default.max_attempts
            } else {
                self.max_attempts
            },
            default_snooze_minutes: if self.default_snooze_minutes <= 0 {
                default.default_snooze_minutes
            } else {
                self.default_snooze_minutes
            },
            event_retention: if self.event_retention <= 0 {
                default.event_retention
            } else {
                self.event_retention
            },
            consumer_liveness: if self.consumer_liveness <= 0 {
                default.consumer_liveness
            } else {
                self.consumer_liveness
            },
            max_lease: if self.max_lease <= 0 {
                default.max_lease
            } else {
                self.max_lease
            },
        }
    }
}

/// A store failure: a domain error rendered exactly like the Go store, the
/// bare `not found` sentinel, or an internal SQLite failure.
#[derive(Debug)]
pub enum StoreError {
    /// Domain failure with a Go-compatible `<kind>: <detail>` message.
    Domain(DomainError),
    /// Bare `not found`, matching `store.ErrNotFound`.
    NotFound,
    /// Bare `consumer not allowed`, matching `store.ErrConsumerDenied`.
    ConsumerDenied,
    /// SQLite failure. The API maps it to `internal error`.
    Sqlite(SqliteError),
}

impl StoreError {
    /// `invalid argument: <detail>`.
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::invalid(detail))
    }

    /// `precondition failed: <detail>`.
    pub fn precondition(detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::precondition(detail))
    }

    /// `limit exceeded: <detail>`.
    pub fn limit(detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::limit_exceeded(detail))
    }

    /// Domain error of an explicit kind.
    pub fn domain(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::new(kind, detail))
    }

    /// Classifies the error for transport mapping.
    #[must_use]
    pub fn kind(&self) -> Option<ErrorKind> {
        match self {
            Self::Domain(error) => Some(error.kind()),
            Self::NotFound => Some(ErrorKind::NotFound),
            Self::ConsumerDenied => Some(ErrorKind::ConsumerDenied),
            Self::Sqlite(_) => None,
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::NotFound => formatter.write_str("not found"),
            Self::ConsumerDenied => formatter.write_str("consumer not allowed"),
            Self::Sqlite(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<DomainError> for StoreError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<SqliteError> for StoreError {
    fn from(error: SqliteError) -> Self {
        Self::Sqlite(error)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(SqliteError::from(error))
    }
}

/// SQLite-backed scheduler store.
pub struct Store {
    db: Mutex<Db>,
    config: Config,
}

impl fmt::Debug for Store {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Store")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Store {
    /// Opens (creating if needed) the database and applies migrations.
    pub fn open(path: impl AsRef<std::path::Path>, config: Config) -> Result<Self, StoreError> {
        let db = Db::open(path, MIGRATIONS)?;
        Ok(Self {
            db: Mutex::new(db),
            config: config.normalized(),
        })
    }

    /// Opens an in-memory database with the same pragmas and migrations.
    pub fn open_in_memory(config: Config) -> Result<Self, StoreError> {
        let db = Db::open_in_memory(MIGRATIONS)?;
        Ok(Self {
            db: Mutex::new(db),
            config: config.normalized(),
        })
    }

    /// Normalized operational bounds.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
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

    fn read<T>(
        &self,
        query: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError>
    where
        T: Send + 'static,
    {
        self.call(move |connection| Ok(query(connection)))?
    }

    fn with_tx<T>(
        &self,
        query: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError>
    where
        T: Send + 'static,
    {
        self.call(move |connection| {
            let transaction = connection.transaction()?;
            match query(&transaction) {
                Ok(value) => {
                    transaction.commit()?;
                    Ok(Ok(value))
                }
                Err(error) => {
                    transaction.rollback()?;
                    Ok(Err(error))
                }
            }
        })?
    }

    /// Stops the writer thread and closes the connection.
    pub fn close(self) -> Result<(), StoreError> {
        let db = self.db.into_inner().unwrap_or_else(PoisonError::into_inner);
        db.shutdown().map_err(StoreError::from)
    }
}

pub(crate) fn weekdays_text(days: &[i32]) -> String {
    crate::domain::weekdays_text(days)
}

pub(crate) fn weekdays_from_text(text: &str) -> Vec<i32> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(',')
        .filter_map(|part| part.trim().parse::<i32>().ok())
        .collect()
}

pub(crate) const fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

pub(crate) fn kind_code(kind: DomainScheduleKind) -> i64 {
    i64::from(kind.code())
}

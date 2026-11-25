mod access;
mod event;
mod migrate;
mod notifications;
mod push;
mod scenario;
mod schedule;
mod status;
mod task;
mod tick;
mod timer;
mod vpn;

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
pub use access::{AccessToken, LEGACY_TOKEN_ID, MAX_APP_TOKENS, OWNER_TOKEN_ID};
pub use notifications::{
    DEFAULT_HISTORY_LIMIT, MAX_HISTORY_LIMIT, Notification, NotificationInput, NotificationKind,
    NotificationPage,
};
pub use push::{DeviceScope, PushDevice, PushDeviceInput};
pub use status::{ConsumerStatus, QueueInfo, Status};
pub use tick::TickStats;
pub use vpn::{VpnPeer, VpnPeerInput, VpnPool, is_wireguard_key as vpn_key_is_valid};

use crate::domain::ScheduleKind as DomainScheduleKind;

pub const SCHEMA_VERSION: i64 = 11;

pub const QUEUE_HOME: &str = "home";
pub const QUEUE_TASKS: &str = "tasks";
pub const QUEUE_SCENARIO: &str = "scenario";

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

const V4: &[&str] = &[
    "ALTER TABLE push_devices RENAME TO push_devices_v3",
    "CREATE TABLE IF NOT EXISTS push_devices (
			device_id TEXT PRIMARY KEY,
			token TEXT NOT NULL,
			platform TEXT NOT NULL,
			app_version TEXT NOT NULL,
			device_name TEXT NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL
		)",
    "INSERT INTO push_devices (device_id, token, platform, app_version, device_name, created_unix, updated_unix)
		SELECT token, token, platform, app_version, device_name, created_unix, updated_unix
		FROM push_devices_v3",
    "DROP TABLE push_devices_v3",
    "CREATE UNIQUE INDEX IF NOT EXISTS push_devices_token ON push_devices(token)",
    "CREATE INDEX IF NOT EXISTS push_devices_updated ON push_devices(updated_unix)",
];

const V5: &[&str] = &["ALTER TABLE schedules ADD COLUMN once_unix INTEGER NOT NULL DEFAULT 0"];

const V6: &[&str] = &["CREATE TABLE IF NOT EXISTS vpn_peers (
			device_id TEXT PRIMARY KEY,
			public_key TEXT NOT NULL UNIQUE,
			host INTEGER NOT NULL UNIQUE,
			device_name TEXT NOT NULL,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL
		)"];

const V7: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS notifications (
			id TEXT PRIMARY KEY,
			device_id TEXT NOT NULL,
			type TEXT NOT NULL,
			title TEXT NOT NULL,
			body TEXT NOT NULL,
			deep_link TEXT,
			created_unix INTEGER NOT NULL,
			read_unix INTEGER NOT NULL DEFAULT 0
		)",
    "CREATE INDEX IF NOT EXISTS notifications_device ON notifications(device_id, created_unix DESC)",
];

const V10: &[&str] = &[
    "ALTER TABLE vpn_peers ADD COLUMN token_id TEXT NOT NULL DEFAULT ''",
    "CREATE INDEX IF NOT EXISTS vpn_peers_owner ON vpn_peers(token_id)",
];

const V11: &[&str] = &["UPDATE push_devices SET device_name = (
			SELECT name FROM app_tokens WHERE app_tokens.id = push_devices.token_id
		)
		WHERE token_id IN (SELECT id FROM app_tokens WHERE revoked_unix = 0)"];

const V9: &[&str] = &[
    "ALTER TABLE push_devices ADD COLUMN token_id TEXT NOT NULL DEFAULT ''",
    "CREATE INDEX IF NOT EXISTS push_devices_owner ON push_devices(token_id, updated_unix)",
];

const V8: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS app_tokens (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL UNIQUE,
			sha256 TEXT NOT NULL UNIQUE,
			modules_json TEXT NOT NULL,
			is_owner INTEGER NOT NULL DEFAULT 0,
			created_unix INTEGER NOT NULL,
			updated_unix INTEGER NOT NULL,
			revoked_unix INTEGER NOT NULL DEFAULT 0
		)",
    "CREATE INDEX IF NOT EXISTS app_tokens_active ON app_tokens(revoked_unix, created_unix)",
    "CREATE TABLE IF NOT EXISTS access_meta (
			id INTEGER PRIMARY KEY CHECK (id = 1),
			revision INTEGER NOT NULL DEFAULT 0,
			legacy_imported INTEGER NOT NULL DEFAULT 0
		)",
    "INSERT OR IGNORE INTO access_meta (id, revision, legacy_imported) VALUES (1, 0, 0)",
];

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
        version: 3,
        description: "scheduler v3 push devices",
        statements: V3,
    },
    Migration {
        version: 4,
        description: "scheduler v4 push device identity",
        statements: V4,
    },
    Migration {
        version: 5,
        description: "scheduler v5 UTC-only schedules",
        statements: V5,
    },
    Migration {
        version: 6,
        description: "scheduler v6 WireGuard peers",
        statements: V6,
    },
    Migration {
        version: 7,
        description: "scheduler v7 notification history",
        statements: V7,
    },
    Migration {
        version: 8,
        description: "scheduler v8 application access tokens",
        statements: V8,
    },
    Migration {
        version: 9,
        description: "scheduler v9 push device ownership",
        statements: V9,
    },
    Migration {
        version: 10,
        description: "scheduler v10 vpn peer ownership",
        statements: V10,
    },
    Migration {
        version: 11,
        description: "scheduler v11 push device names from application tokens",
        statements: V11,
    },
];

#[derive(Debug, Clone)]
pub struct Config {
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

#[derive(Debug)]
pub enum StoreError {
    Domain(DomainError),
    NotFound,
    ConsumerDenied,
    Sqlite(SqliteError),
}

impl StoreError {
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::invalid(detail))
    }

    pub fn precondition(detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::precondition(detail))
    }

    pub fn limit(detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::limit_exceeded(detail))
    }

    pub fn domain(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self::Domain(DomainError::new(kind, detail))
    }

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
    pub fn open(path: impl AsRef<std::path::Path>, config: Config) -> Result<Self, StoreError> {
        let db = Db::open(path, MIGRATIONS)?;
        migrate::legacy_schedules_to_utc(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            config: config.normalized(),
        })
    }

    pub fn open_in_memory(config: Config) -> Result<Self, StoreError> {
        let db = Db::open_in_memory(MIGRATIONS)?;
        migrate::legacy_schedules_to_utc(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            config: config.normalized(),
        })
    }

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

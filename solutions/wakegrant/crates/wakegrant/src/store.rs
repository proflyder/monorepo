//! Durable grant storage over the shared single-writer SQLite actor.
//!
//! The schema is the Go service's schema version 2 (`schema_meta`), so an
//! existing PVC database opens in place and no table is recreated.

use std::path::Path;
use std::sync::Arc;

use home_sqlite::{Db, Migration, SqliteError};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::Purpose;

/// Current schema version, matching the Go `wakegrant` store.
pub const SCHEMA_VERSION: i64 = 2;

const V1: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS grants (
        id TEXT PRIMARY KEY,
        machine TEXT NOT NULL,
        created_unix INTEGER NOT NULL,
        expires_unix INTEGER NOT NULL,
        consumed INTEGER NOT NULL DEFAULT 0,
        consumed_unix INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE INDEX IF NOT EXISTS grants_machine ON grants(machine, consumed, expires_unix)",
];

const V2: &[&str] = &[
    "ALTER TABLE grants ADD COLUMN purpose TEXT NOT NULL DEFAULT 'logon'",
    "CREATE INDEX IF NOT EXISTS grants_machine_purpose ON grants(machine, purpose, consumed, expires_unix)",
];

/// Ordered Go-compatible migrations.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "grants",
        statements: V1,
    },
    Migration {
        version: 2,
        description: "grants.purpose",
        statements: V2,
    },
];

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),
    #[error("database path is required")]
    MissingPath,
}

/// An issued grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: String,
    pub expires_at: OffsetDateTime,
}

/// Durable grant store.
pub struct Store {
    db: Arc<Db>,
}

impl Store {
    /// Opens (creating if needed) and migrates the database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(StoreError::MissingPath);
        }
        Ok(Self {
            db: Arc::new(Db::open(path, MIGRATIONS)?),
        })
    }

    /// In-memory store for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Ok(Self {
            db: Arc::new(Db::open_in_memory(MIGRATIONS)?),
        })
    }

    /// Stores a new grant and replaces any unconsumed grant for the same
    /// `(machine, purpose)`, so only the newest one of that kind can be
    /// claimed. A grant of the other purpose is untouched.
    pub async fn create_grant(
        &self,
        machine: &str,
        purpose: Purpose,
        now: OffsetDateTime,
        ttl: std::time::Duration,
    ) -> Result<Grant, StoreError> {
        let id = Uuid::new_v4().to_string();
        let expires_at = now + ttl;
        let created_nanos = now.unix_timestamp_nanos() as i64;
        let expires_secs = expires_at.unix_timestamp();
        let machine = machine.to_owned();
        let purpose_text = purpose.as_str();
        let stored_id = id.clone();
        self.db
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute(
                    "DELETE FROM grants WHERE machine = ?1 AND purpose = ?2 AND consumed = 0",
                    rusqlite::params![machine, purpose_text],
                )?;
                transaction.execute(
                    "INSERT INTO grants(id, machine, purpose, created_unix, expires_unix, consumed)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                    rusqlite::params![
                        stored_id,
                        machine,
                        purpose_text,
                        created_nanos,
                        expires_secs
                    ],
                )?;
                transaction.commit()
            })
            .await?;
        Ok(Grant { id, expires_at })
    }

    /// Atomically consumes the newest unexpired unconsumed grant for
    /// `(machine, purpose)`. `None` means there was nothing to grant.
    pub async fn consume_newest(
        &self,
        machine: &str,
        purpose: Purpose,
        now: OffsetDateTime,
    ) -> Result<Option<String>, StoreError> {
        let now_nanos = now.unix_timestamp_nanos() as i64;
        let now_secs = now.unix_timestamp();
        let machine = machine.to_owned();
        let purpose_text = purpose.as_str();
        let result = self
            .db
            .call(move |connection| {
                connection.query_row(
                    "UPDATE grants SET consumed = 1, consumed_unix = ?1
                     WHERE rowid = (
                        SELECT rowid FROM grants
                        WHERE machine = ?2 AND purpose = ?3 AND consumed = 0 AND expires_unix > ?4
                        ORDER BY rowid DESC LIMIT 1
                     )
                     RETURNING id",
                    rusqlite::params![now_nanos, machine, purpose_text, now_secs],
                    |row| row.get::<_, String>(0),
                )
            })
            .await;
        match result {
            Ok(id) => Ok(Some(id)),
            Err(SqliteError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Health check used by `/healthz`.
    pub async fn ping(&self) -> Result<(), StoreError> {
        self.db.ping().await?;
        Ok(())
    }

    /// Removes consumed and expired grants.
    pub async fn cleanup(&self, now: OffsetDateTime) -> Result<usize, StoreError> {
        let now_secs = now.unix_timestamp();
        let removed = self
            .db
            .call(move |connection| {
                connection.execute(
                    "DELETE FROM grants WHERE consumed = 1 OR expires_unix <= ?1",
                    [now_secs],
                )
            })
            .await?;
        Ok(removed)
    }

    /// Number of rows (tests only).
    #[cfg(test)]
    pub(crate) async fn count(&self) -> Result<i64, StoreError> {
        let count = self
            .db
            .call(|connection| {
                connection.query_row("SELECT COUNT(*) FROM grants", [], |row| row.get(0))
            })
            .await?;
        Ok(count)
    }

    /// Seeds the published PROTOCOL.md vector (tests only).
    #[cfg(test)]
    pub(crate) fn insert_grant_for_test(
        &self,
        id: &str,
        machine: &str,
        purpose: Purpose,
        now: OffsetDateTime,
        ttl: std::time::Duration,
    ) -> Result<(), StoreError> {
        let id = id.to_owned();
        let machine = machine.to_owned();
        let purpose = purpose.as_str();
        let created = now.unix_timestamp_nanos() as i64;
        let expires = (now + ttl).unix_timestamp();
        self.db.call_blocking(move |connection| {
            connection.execute(
                "INSERT INTO grants(id, machine, purpose, created_unix, expires_unix, consumed)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                rusqlite::params![id, machine, purpose, created, expires],
            )
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    const NOW: OffsetDateTime = datetime!(2026-11-03 12:00:00 UTC);

    #[tokio::test]
    async fn newest_grant_wins_within_a_purpose() {
        let store = Store::open_in_memory().expect("store");
        store
            .create_grant(
                "komp",
                Purpose::Logon,
                NOW,
                std::time::Duration::from_secs(180),
            )
            .await
            .expect("first");
        let second = store
            .create_grant(
                "komp",
                Purpose::Logon,
                NOW,
                std::time::Duration::from_secs(180),
            )
            .await
            .expect("second");
        assert_eq!(
            store
                .consume_newest("komp", Purpose::Logon, NOW)
                .await
                .expect("consume")
                .as_deref(),
            Some(second.id.as_str())
        );
        assert_eq!(
            store
                .consume_newest("komp", Purpose::Logon, NOW)
                .await
                .expect("second consume"),
            None
        );
    }

    #[tokio::test]
    async fn purposes_are_isolated_and_expired_grants_are_ignored() {
        let store = Store::open_in_memory().expect("store");
        let logon = store
            .create_grant(
                "komp",
                Purpose::Logon,
                NOW,
                std::time::Duration::from_secs(180),
            )
            .await
            .expect("logon");
        let unlock = store
            .create_grant(
                "komp",
                Purpose::Unlock,
                NOW,
                std::time::Duration::from_secs(30),
            )
            .await
            .expect("unlock");

        assert_eq!(
            store
                .consume_newest("komp", Purpose::Unlock, NOW + time::Duration::seconds(31))
                .await
                .expect("expired unlock"),
            None
        );
        assert_eq!(
            store
                .consume_newest("komp", Purpose::Logon, NOW)
                .await
                .expect("logon"),
            Some(logon.id)
        );
        assert_eq!(unlock.id.len(), 36);
    }

    #[tokio::test]
    async fn cleanup_removes_consumed_and_expired_rows() {
        let store = Store::open_in_memory().expect("store");
        store
            .create_grant(
                "komp",
                Purpose::Logon,
                NOW,
                std::time::Duration::from_secs(30),
            )
            .await
            .expect("grant");
        store
            .consume_newest("komp", Purpose::Logon, NOW)
            .await
            .expect("consume");
        store
            .create_grant(
                "komp",
                Purpose::Logon,
                NOW,
                std::time::Duration::from_secs(30),
            )
            .await
            .expect("second");
        assert_eq!(
            store
                .cleanup(NOW + time::Duration::seconds(31))
                .await
                .expect("cleanup"),
            2
        );
        assert_eq!(store.count().await.expect("count"), 0);
    }
}

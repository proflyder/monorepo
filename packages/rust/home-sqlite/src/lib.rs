//! One SQLite connection owned by one thread.
//!
//! Every query goes through [`Db::call`], which hands the closure to the
//! writer thread and returns its result through a oneshot channel. That is the
//! same serialization the Go services get from `SetMaxOpenConns(1)`: two
//! concurrent claims cannot observe the same unconsumed row.
//!
//! Schema versions live in `schema_meta(version)` — the same table the Go
//! `wakegrant` store uses. [`Migration`]s are applied in order and an existing
//! database at a known version is left untouched, so a PVC created by the Go
//! service opens in place.

use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use rusqlite::Connection;
use thiserror::Error;
use tokio::sync::oneshot;

/// One ordered schema step. `version` must increase by one from 1.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub statements: &'static [&'static str],
}

/// SQLite failure around the actor boundary.
#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("open {path}: {source}")]
    Open {
        path: String,
        source: rusqlite::Error,
    },
    #[error("database schema version {found} is newer than supported {supported}")]
    NewerSchema { found: i64, supported: i64 },
    #[error("migration {version} ({description}): {source}")]
    Migration {
        version: i64,
        description: &'static str,
        source: rusqlite::Error,
    },
    #[error("sqlite writer thread is gone")]
    Closed,
    #[error("sqlite writer thread panicked")]
    Panicked,
}

type Job = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

/// Handle to the single writer thread.
pub struct Db {
    jobs: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Db {
    /// Opens `path` (or an in-memory database for `:memory:`), applies the
    /// Go-compatible pragmas and the pending migrations before returning.
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration]) -> Result<Self, SqliteError> {
        let path = path.as_ref();
        let display = path.display().to_string();
        let mut connection = Connection::open(path).map_err(|source| SqliteError::Open {
            path: display,
            source,
        })?;
        configure(&connection)?;
        migrate(&mut connection, migrations)?;
        Ok(Self::from_connection(connection))
    }

    /// In-memory database with the same pragmas and migrations.
    pub fn open_in_memory(migrations: &[Migration]) -> Result<Self, SqliteError> {
        let mut connection = Connection::open_in_memory()?;
        configure(&connection)?;
        migrate(&mut connection, migrations)?;
        Ok(Self::from_connection(connection))
    }

    fn from_connection(connection: Connection) -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        let thread = thread::Builder::new()
            .name("home-sqlite".to_owned())
            .spawn(move || {
                let mut connection = connection;
                while let Ok(job) = receiver.recv() {
                    job(&mut connection);
                }
            })
            .expect("spawning the SQLite writer thread cannot fail");
        Self {
            jobs: Some(jobs),
            thread: Some(thread),
        }
    }

    /// Runs `query` on the writer thread and awaits its result.
    pub async fn call<F, R>(&self, query: F) -> Result<R, SqliteError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        let job: Job = Box::new(move |connection| {
            let _ = sender.send(query(connection));
        });
        self.jobs
            .as_ref()
            .ok_or(SqliteError::Closed)?
            .send(job)
            .map_err(|_| SqliteError::Closed)?;
        receiver
            .await
            .map_err(|_| SqliteError::Closed)?
            .map_err(SqliteError::from)
    }

    /// Blocking variant of [`Db::call`] for migrations and test seeding.
    pub fn call_blocking<F, R>(&self, query: F) -> Result<R, SqliteError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        let job: Job = Box::new(move |connection| {
            let _ = sender.send(query(connection));
        });
        self.jobs
            .as_ref()
            .ok_or(SqliteError::Closed)?
            .send(job)
            .map_err(|_| SqliteError::Closed)?;
        receiver
            .recv()
            .map_err(|_| SqliteError::Closed)?
            .map_err(SqliteError::from)
    }

    /// Health check used by `/healthz`.
    pub async fn ping(&self) -> Result<(), SqliteError> {
        self.call(|connection| connection.query_row("SELECT 1", [], |_| Ok(())))
            .await
    }

    /// Stops the writer thread and closes the connection.
    pub fn shutdown(mut self) -> Result<(), SqliteError> {
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| SqliteError::Panicked)?;
        }
        Ok(())
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // Dropping the sender ends the loop; joining keeps the connection from
        // outliving the handle. Panics are already surfaced by shutdown().
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn configure(connection: &Connection) -> Result<(), SqliteError> {
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// Applies every migration newer than the stored `schema_meta.version`.
pub fn migrate(connection: &mut Connection, migrations: &[Migration]) -> Result<(), SqliteError> {
    for pair in migrations.windows(2) {
        debug_assert_eq!(
            pair[0].version + 1,
            pair[1].version,
            "migrations must be contiguous"
        );
    }
    let supported = migrations.last().map_or(0, |migration| migration.version);

    let transaction = connection.transaction()?;
    transaction.execute(
        "CREATE TABLE IF NOT EXISTS schema_meta (version INTEGER NOT NULL)",
        [],
    )?;
    let version: Option<i64> = transaction
        .query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
            row.get(0)
        })
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let current = if let Some(version) = version {
        version
    } else {
        transaction.execute("INSERT INTO schema_meta(version) VALUES (0)", [])?;
        0
    };
    if current > supported {
        return Err(SqliteError::NewerSchema {
            found: current,
            supported,
        });
    }

    let mut applied = current;
    for migration in migrations {
        if migration.version <= current {
            continue;
        }
        for statement in migration.statements {
            transaction
                .execute(statement, [])
                .map_err(|source| SqliteError::Migration {
                    version: migration.version,
                    description: migration.description,
                    source,
                })?;
        }
        applied = migration.version;
    }
    transaction.execute("UPDATE schema_meta SET version = ?1", [applied])?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &[&str] = &[
        "CREATE TABLE grants (
            id TEXT PRIMARY KEY,
            machine TEXT NOT NULL,
            created_unix INTEGER NOT NULL,
            expires_unix INTEGER NOT NULL,
            consumed INTEGER NOT NULL DEFAULT 0,
            consumed_unix INTEGER NOT NULL DEFAULT 0
        )",
        "CREATE INDEX grants_machine ON grants(machine, consumed, expires_unix)",
    ];
    const V2: &[&str] = &[
        "ALTER TABLE grants ADD COLUMN purpose TEXT NOT NULL DEFAULT 'logon'",
        "CREATE INDEX grants_machine_purpose ON grants(machine, purpose, consumed, expires_unix)",
    ];
    const MIGRATIONS: &[Migration] = &[
        Migration {
            version: 1,
            description: "grants",
            statements: V1,
        },
        Migration {
            version: 2,
            description: "purpose",
            statements: V2,
        },
    ];

    #[tokio::test]
    async fn fresh_database_reaches_version_two() {
        let db = Db::open_in_memory(MIGRATIONS).expect("open");
        let version: i64 = db
            .call(|connection| {
                connection.query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
                    row.get(0)
                })
            })
            .await
            .expect("version");
        assert_eq!(version, 2);
        db.ping().await.expect("ping");
        db.shutdown().expect("shutdown");
    }

    #[tokio::test]
    async fn a_go_version_two_database_opens_in_place() {
        let path = std::env::temp_dir().join(format!(
            "home-sqlite-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        {
            // Exactly what the Go store's v2 migration leaves behind.
            let connection = Connection::open(&path).expect("create");
            connection
                .execute("CREATE TABLE schema_meta (version INTEGER NOT NULL)", [])
                .expect("schema_meta");
            connection
                .execute("INSERT INTO schema_meta(version) VALUES (2)", [])
                .expect("version");
            for statement in V1 {
                connection.execute(statement, []).expect("v1");
            }
            for statement in V2 {
                connection.execute(statement, []).expect("v2");
            }
            connection
                .execute(
                    "INSERT INTO grants(id, machine, purpose, created_unix, expires_unix, consumed)
                     VALUES ('legacy', 'komp', 'logon', 1, 2, 0)",
                    [],
                )
                .expect("legacy grant");
        }

        let db = Db::open(&path, MIGRATIONS).expect("reopen existing database");
        let (version, purpose): (i64, String) = db
            .call(|connection| {
                let version =
                    connection.query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
                        row.get(0)
                    })?;
                let purpose = connection.query_row(
                    "SELECT purpose FROM grants WHERE id = 'legacy'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((version, purpose))
            })
            .await
            .expect("read");
        assert_eq!(version, 2);
        assert_eq!(purpose, "logon");
        db.shutdown().expect("shutdown");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn calls_are_serialized() {
        let db = std::sync::Arc::new(Db::open_in_memory(MIGRATIONS).expect("open"));
        db.call(|connection| {
            connection.execute("CREATE TABLE counter (value INTEGER NOT NULL)", [])
        })
        .await
        .expect("create");
        db.call(|connection| connection.execute("INSERT INTO counter(value) VALUES (0)", []))
            .await
            .expect("seed");

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let db = std::sync::Arc::clone(&db);
            tasks.push(tokio::spawn(async move {
                db.call(|connection| {
                    connection.execute("UPDATE counter SET value = value + 1", [])
                })
                .await
                .expect("increment");
            }));
        }
        for task in tasks {
            task.await.expect("task");
        }
        let value: i64 = db
            .call(|connection| {
                connection.query_row("SELECT value FROM counter", [], |row| row.get(0))
            })
            .await
            .expect("value");
        assert_eq!(value, 8);
    }
}

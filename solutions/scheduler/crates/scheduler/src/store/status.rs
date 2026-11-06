//! Honest scheduler status counters and retention purge.

use rusqlite::params;

use crate::domain::{EventState, TimerState};

use super::{QUEUE_HOME, QUEUE_SCENARIO, QUEUE_TASKS, SCHEMA_VERSION, Store, StoreError};

/// Liveness of one event consumer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsumerStatus {
    pub consumer_id: String,
    pub last_seen_unix: i64,
    pub last_ack_unix: i64,
    pub alive: bool,
}

/// Durable backlog of one queue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueInfo {
    pub name: String,
    pub pending: i32,
    pub leased: i32,
}

/// Snapshot of scheduler state. It never asserts that sound was audible.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub schema_version: i64,
    pub pending_events: i32,
    pub leased_events: i32,
    pub enabled_schedules: i32,
    pub running_timers: i32,
    pub consumers: Vec<ConsumerStatus>,
    pub queues: Vec<QueueInfo>,
}

impl Store {
    /// Collects counters and consumer liveness at `now_unix`.
    pub fn status(&self, now_unix: i64) -> Result<Status, StoreError> {
        let liveness = self.config().consumer_liveness;
        self.read(move |connection| {
            let mut status = Status {
                schema_version: SCHEMA_VERSION,
                ..Status::default()
            };
            status.pending_events = count(
                connection,
                "SELECT COUNT(*) FROM events WHERE state = 1",
            )?;
            status.leased_events =
                count(connection, "SELECT COUNT(*) FROM events WHERE state = 2")?;
            status.enabled_schedules = count(
                connection,
                "SELECT COUNT(*) FROM schedules WHERE enabled = 1 AND deleted_unix = 0",
            )?;
            status.running_timers =
                count(connection, "SELECT COUNT(*) FROM timers WHERE state IN (1, 2)")?;

            let mut statement = connection.prepare(
                "SELECT consumer_id, last_seen_unix, last_ack_unix FROM consumers ORDER BY consumer_id",
            )?;
            let consumers = statement
                .query_map([], |row| {
                    let consumer_id: String = row.get(0)?;
                    let last_seen_unix: i64 = row.get(1)?;
                    let last_ack_unix: i64 = row.get(2)?;
                    Ok(ConsumerStatus {
                        consumer_id,
                        last_seen_unix,
                        last_ack_unix,
                        alive: last_seen_unix > 0
                            && now_unix.saturating_sub(last_seen_unix) <= liveness,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            status.consumers = consumers;

            let mut statement = connection.prepare(
                "SELECT queue,
                    SUM(CASE WHEN state = 1 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN state = 2 THEN 1 ELSE 0 END)
                    FROM events GROUP BY queue ORDER BY queue",
            )?;
            let mut queues = statement
                .query_map([], |row| {
                    Ok(QueueInfo {
                        name: row.get(0)?,
                        pending: i32::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                        leased: i32::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for name in [QUEUE_HOME, QUEUE_TASKS, QUEUE_SCENARIO] {
                if !queues.iter().any(|queue| queue.name == name) {
                    queues.push(QueueInfo {
                        name: name.to_owned(),
                        pending: 0,
                        leased: 0,
                    });
                }
            }
            queues.sort_by(|left, right| left.name.cmp(&right.name));
            status.queues = queues;
            Ok(status)
        })
    }

    /// Removes terminal events and idempotency rows older than retention.
    pub fn purge(&self, now_unix: i64) -> Result<i64, StoreError> {
        let cutoff = now_unix.saturating_sub(self.config().event_retention);
        self.with_tx(move |tx| {
            let removed = tx.execute(
                "DELETE FROM events WHERE state IN (?, ?, ?) AND created_unix < ?",
                params![
                    i64::from(EventState::Delivered.code()),
                    i64::from(EventState::Failed.code()),
                    i64::from(EventState::Skipped.code()),
                    cutoff,
                ],
            )?;
            tx.execute(
                "DELETE FROM idempotency WHERE created_unix < ?",
                params![cutoff],
            )?;
            Ok(i64::try_from(removed).unwrap_or(0))
        })
    }
}

fn count(connection: &rusqlite::Connection, query: &str) -> Result<i32, StoreError> {
    let value: i64 = connection.query_row(query, [], |row| row.get(0))?;
    Ok(i32::try_from(value).unwrap_or(i32::MAX))
}

// Keep the state/timer codes referenced so the status SQL and the domain enum
// stay visibly coupled.
const _: i32 = EventState::Pending.code();
const _: i32 = TimerState::Running.code();

//! Durable countdown timers.

use rusqlite::{Row, Transaction, params};

use crate::domain::{
    EventAction, EventState, SourceKind, Timer, TimerState, canonical_timer, hash::sha1_hex,
    resource_id, validate_duration, validate_id, validate_idempotency_key, validate_label,
};

use super::event::{EventInsert, insert_event};
use super::{Store, StoreError};

pub const TIMER_COLS: &str = "id, label, duration_seconds, remaining_seconds, due_unix,
	state, revision, created_unix, updated_unix";

pub fn scan_timer(row: &Row<'_>) -> rusqlite::Result<Timer> {
    let state: i64 = row.get(5)?;
    Ok(Timer {
        id: row.get(0)?,
        label: row.get(1)?,
        duration_seconds: row.get(2)?,
        remaining_seconds: row.get(3)?,
        due_unix: row.get(4)?,
        state: TimerState::from_code(i32::try_from(state).unwrap_or(0)).unwrap_or_default(),
        revision: row.get(6)?,
        created_unix: row.get(7)?,
        updated_unix: row.get(8)?,
    })
}

fn get_timer_tx(tx: &Transaction<'_>, id: &str) -> Result<Timer, StoreError> {
    match tx.query_row(
        &format!("SELECT {TIMER_COLS} FROM timers WHERE id=?"),
        params![id],
        scan_timer,
    ) {
        Ok(timer) => Ok(timer),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
        Err(error) => Err(error.into()),
    }
}

fn idempotency_lookup(
    tx: &Transaction<'_>,
    key: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    match tx.query_row(
        "SELECT resource_kind, resource_id, request_hash FROM idempotency WHERE key = ?",
        params![key],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

impl Store {
    /// Starts a durable countdown; a non-empty idempotency key replays the
    /// original timer when the request matches.
    pub fn start_timer(
        &self,
        label: &str,
        duration_seconds: i64,
        idempotency_key: &str,
        now_unix: i64,
    ) -> Result<Timer, StoreError> {
        validate_label(label)?;
        validate_duration(duration_seconds, self.config().max_timer_duration)?;
        validate_idempotency_key(idempotency_key)?;
        let id = resource_id(now_unix.saturating_mul(1_000));
        let hash = sha1_hex(&canonical_timer(label, duration_seconds));
        let key = idempotency_key.to_owned();
        let label = label.to_owned();
        let max_timers = self.config().max_timers;
        let timer = Timer {
            id,
            label,
            duration_seconds,
            remaining_seconds: 0,
            due_unix: now_unix.saturating_add(duration_seconds),
            state: TimerState::Running,
            revision: 1,
            created_unix: now_unix,
            updated_unix: now_unix,
        };
        let row = timer;
        self.with_tx(move |tx| {
            if !key.is_empty()
                && let Some((kind, resource_id, existing_hash)) = idempotency_lookup(tx, &key)?
            {
                    if kind != "timer" || existing_hash != hash {
                        return Err(StoreError::domain(
                            crate::domain::ErrorKind::Idempotency,
                            "idempotency key reused with a different request",
                        ));
                    }
                    let timer = get_timer_tx(tx, &resource_id).map_err(|error| {
                        if matches!(error, StoreError::NotFound) {
                            StoreError::domain(
                                crate::domain::ErrorKind::Idempotency,
                                "idempotent timer no longer exists",
                            )
                        } else {
                            error
                        }
                    })?;
                    return Ok(timer);
            }
            let active: i64 = tx.query_row(
                "SELECT COUNT(*) FROM timers WHERE state IN (?, ?)",
                params![
                    i64::from(TimerState::Running.code()),
                    i64::from(TimerState::Paused.code()),
                ],
                |row| row.get(0),
            )?;
            if active >= max_timers {
                return Err(StoreError::limit(format!(
                    "at most {max_timers} active timers"
                )));
            }
            tx.execute(
                &format!("INSERT INTO timers ({TIMER_COLS}) VALUES (?,?,?,?,?,?,?,?,?)"),
                params![
                    row.id,
                    row.label,
                    row.duration_seconds,
                    row.remaining_seconds,
                    row.due_unix,
                    i64::from(row.state.code()),
                    row.revision,
                    row.created_unix,
                    row.updated_unix,
                ],
            )?;
            if !key.is_empty() {
                tx.execute(
                    r"INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)",
                    params![key, "timer", row.id, hash, now_unix],
                )
                .map_err(|_| {
                    StoreError::domain(
                        crate::domain::ErrorKind::Idempotency,
                        "idempotency key already exists",
                    )
                })?;
            }
            Ok(row)
        })
    }

    /// Returns a timer by id.
    pub fn get_timer(&self, id: &str) -> Result<Timer, StoreError> {
        validate_id(id)?;
        let id = id.to_owned();
        self.read(move |connection| {
            match connection.query_row(
                &format!("SELECT {TIMER_COLS} FROM timers WHERE id=?"),
                params![id],
                scan_timer,
            ) {
                Ok(timer) => Ok(timer),
                Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
                Err(error) => Err(error.into()),
            }
        })
    }

    /// Timers ordered by creation time.
    pub fn list_timers(&self, include_finished: bool) -> Result<Vec<Timer>, StoreError> {
        self.read(move |connection| {
            let mut query = format!("SELECT {TIMER_COLS} FROM timers");
            if !include_finished {
                query.push_str(" WHERE state IN (1, 2)");
            }
            query.push_str(" ORDER BY created_unix, id");
            let mut statement = connection.prepare(&query)?;
            let rows = statement.query_map([], scan_timer)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        })
    }

    /// Stores the remaining duration and pauses the timer.
    pub fn pause_timer(&self, id: &str, now_unix: i64) -> Result<Timer, StoreError> {
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let mut timer = get_timer_tx(tx, &id)?;
            match timer.state {
                TimerState::Paused => return Ok(timer),
                TimerState::Running => {}
                other => {
                    return Err(StoreError::precondition(format!(
                        "timer is {}",
                        other.lowercase_name()
                    )));
                }
            }
            let remaining = timer.due_unix.saturating_sub(now_unix).max(0);
            timer.remaining_seconds = remaining;
            timer.due_unix = 0;
            timer.state = TimerState::Paused;
            timer.revision += 1;
            timer.updated_unix = now_unix;
            tx.execute(
                "UPDATE timers SET remaining_seconds=?, due_unix=0, state=?,
                    revision=?, updated_unix=? WHERE id=?",
                params![
                    timer.remaining_seconds,
                    i64::from(timer.state.code()),
                    timer.revision,
                    timer.updated_unix,
                    timer.id,
                ],
            )?;
            Ok(timer)
        })
    }

    /// Continues a paused timer from its stored remaining duration.
    pub fn resume_timer(&self, id: &str, now_unix: i64) -> Result<Timer, StoreError> {
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let mut timer = get_timer_tx(tx, &id)?;
            match timer.state {
                TimerState::Running => return Ok(timer),
                TimerState::Paused => {}
                other => {
                    return Err(StoreError::precondition(format!(
                        "timer is {}",
                        other.lowercase_name()
                    )));
                }
            }
            timer.due_unix = now_unix.saturating_add(timer.remaining_seconds);
            timer.state = TimerState::Running;
            timer.remaining_seconds = 0;
            timer.revision += 1;
            timer.updated_unix = now_unix;
            tx.execute(
                "UPDATE timers SET remaining_seconds=0, due_unix=?, state=?,
                    revision=?, updated_unix=? WHERE id=?",
                params![
                    timer.due_unix,
                    i64::from(timer.state.code()),
                    timer.revision,
                    timer.updated_unix,
                    timer.id,
                ],
            )?;
            Ok(timer)
        })
    }

    /// Cancels a running or paused timer, or sends STOP for a fired timer.
    pub fn cancel_timer(&self, id: &str, now_unix: i64) -> Result<Timer, StoreError> {
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let mut timer = get_timer_tx(tx, &id)?;
            match timer.state {
                TimerState::Cancelled => return Ok(timer),
                TimerState::Fired => {
                    insert_event(
                        tx,
                        EventInsert {
                            dedupe_key: crate::domain::timer_control_stop_key(
                                &timer.id,
                                timer.due_unix,
                            ),
                            source_id: timer.id.clone(),
                            source_kind: SourceKind::Timer,
                            action: EventAction::Stop,
                            due_unix: now_unix,
                            occurrence_unix: 0,
                            label: timer.label.clone(),
                            payload_json: "{}".to_owned(),
                            state: EventState::Pending,
                            detail: "cancelled".to_owned(),
                            created_unix: now_unix,
                            ..EventInsert::default()
                        },
                    )?;
                }
                TimerState::Running | TimerState::Paused => {}
                other => {
                    return Err(StoreError::precondition(format!(
                        "timer is {}",
                        other.lowercase_name()
                    )));
                }
            }
            timer.state = TimerState::Cancelled;
            timer.due_unix = 0;
            timer.remaining_seconds = 0;
            timer.revision += 1;
            timer.updated_unix = now_unix;
            tx.execute(
                "UPDATE timers SET state=?, due_unix=0, remaining_seconds=0,
                    revision=?, updated_unix=? WHERE id=?",
                params![
                    i64::from(timer.state.code()),
                    timer.revision,
                    timer.updated_unix,
                    timer.id,
                ],
            )?;
            Ok(timer)
        })
    }
}

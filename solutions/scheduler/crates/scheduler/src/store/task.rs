//! Generic one-shot task events in the shared outbox.

use rusqlite::params;

use crate::domain::{
    AckOutcome, ErrorKind, Event, EventAction, EventState, MAX_TASK_DELAY_SECONDS, SourceKind,
    canonical_task, hash::sha1_hex, resource_id, task_dedupe_key, validate_event_id,
    validate_idempotency_key, validate_payload, validate_queue, validate_run_at,
    validate_task_type,
};

use super::event::{EVENT_COLS, EventInsert, insert_event, scan_event};
use super::{QUEUE_TASKS, Store, StoreError};

impl Store {
    /// Durably queues a generic task event. It is one-shot; recurring generic
    /// task schedules are not implemented.
    pub fn enqueue_task(
        &self,
        task_type: &str,
        payload_json: &str,
        queue: &str,
        run_at_unix: i64,
        idempotency_key: &str,
        now_unix: i64,
    ) -> Result<Event, StoreError> {
        validate_task_type(task_type)?;
        let queue = if queue.trim().is_empty() {
            QUEUE_TASKS
        } else {
            queue
        };
        validate_queue(queue)?;
        let payload = if payload_json.is_empty() {
            "{}"
        } else {
            payload_json
        };
        validate_payload(payload, self.config().max_payload_bytes)?;
        let due_unix = validate_run_at(run_at_unix, now_unix, MAX_TASK_DELAY_SECONDS)?;
        validate_idempotency_key(idempotency_key)?;
        let hash = sha1_hex(&canonical_task(task_type, queue, payload, run_at_unix));
        let task_id = resource_id(now_unix.saturating_mul(1_000));
        let key = idempotency_key.to_owned();
        let task_type = task_type.to_owned();
        let payload = payload.to_owned();
        let queue = queue.to_owned();
        self.with_tx(move |tx| {
            if !key.is_empty()
                && let Some((kind, resource_id, existing_hash)) = idempotency_lookup(tx, &key)?
            {
                    if kind != "task" || existing_hash != hash {
                        return Err(StoreError::domain(
                            ErrorKind::Idempotency,
                            "idempotency key reused with a different request",
                        ));
                    }
                    let sql = format!("SELECT {EVENT_COLS} FROM events WHERE id=?");
                    return match tx.query_row(&sql, params![resource_id], scan_event) {
                        Ok(row) => Ok(row.event),
                        Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::domain(
                            ErrorKind::Idempotency,
                            "idempotent task no longer exists",
                        )),
                        Err(error) => Err(error.into()),
                    };
            }
            let event = insert_event(
                tx,
                EventInsert {
                    dedupe_key: task_dedupe_key(&task_id),
                    source_id: task_id.clone(),
                    source_kind: SourceKind::Task,
                    action: EventAction::Fire,
                    due_unix,
                    occurrence_unix: 0,
                    label: String::new(),
                    payload_json: payload,
                    state: EventState::Pending,
                    created_unix: now_unix,
                    task_type,
                    queue,
                    ..EventInsert::default()
                },
            )?;
            if !key.is_empty() {
                tx.execute(
                    r"INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)",
                    params![key, "task", event.id, hash, now_unix],
                )
                .map_err(|_| {
                    StoreError::domain(ErrorKind::Idempotency, "idempotency key already exists")
                })?;
            }
            Ok(event)
        })
    }

    /// Cancels a task event that is still pending. A leased task is a
    /// precondition error; terminal events replay unchanged.
    pub fn cancel_task(&self, event_id: &str, now_unix: i64) -> Result<Event, StoreError> {
        validate_event_id(event_id)?;
        let event_id = event_id.to_owned();
        self.with_tx(move |tx| {
            let sql = format!("SELECT {EVENT_COLS} FROM events WHERE id=?");
            let row = match tx.query_row(&sql, params![event_id], scan_event) {
                Ok(row) => row,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Err(StoreError::NotFound),
                Err(error) => return Err(error.into()),
            };
            let event = row.event;
            if event.source_kind != SourceKind::Task {
                return Err(StoreError::precondition(
                    "only task events can be cancelled; use StopSchedule",
                ));
            }
            match event.state {
                EventState::Delivered | EventState::Failed | EventState::Skipped => {
                    return Ok(event);
                }
                EventState::Leased => {
                    return Err(StoreError::precondition(
                        "task is already leased by an executor",
                    ));
                }
                _ => {}
            }
            tx.execute(
                "UPDATE events SET state=?, acked_unix=?, ack_outcome=?,
                    lease_owner='', lease_until_unix=0, detail='cancelled' WHERE id=?",
                params![
                    i64::from(EventState::Skipped.code()),
                    now_unix,
                    i64::from(AckOutcome::Skipped.code()),
                    event_id,
                ],
            )?;
            let sql = format!("SELECT {EVENT_COLS} FROM events WHERE id=?");
            Ok(tx.query_row(&sql, params![event_id], scan_event)?.event)
        })
    }
}

fn idempotency_lookup(
    tx: &rusqlite::Transaction<'_>,
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

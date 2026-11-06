//! Outbox events, leases and acknowledgements.

use rusqlite::{Row, Transaction, params};

use crate::domain::{
    AckOutcome, Event, EventAction, EventState, SourceKind, default_queue, default_task_type,
    event_id, validate_detail, validate_event_id, validate_queue,
};

use super::{Store, StoreError};

pub const EVENT_COLS: &str = "id, dedupe_key, source_id, source_kind, action, due_unix,
	occurrence_unix, label, payload_json, state, lease_owner, lease_until_unix,
	attempts, snooze_seq, detail, created_unix, acked_unix, ack_outcome,
	task_type, queue";

#[derive(Debug, Default)]
pub struct EventInsert {
    pub dedupe_key: String,
    pub source_id: String,
    pub source_kind: SourceKind,
    pub action: EventAction,
    pub due_unix: i64,
    pub occurrence_unix: i64,
    pub label: String,
    pub payload_json: String,
    pub state: EventState,
    pub detail: String,
    pub snooze_seq: i64,
    pub created_unix: i64,
    pub acked_unix: i64,
    pub ack_outcome: AckOutcome,
    pub task_type: String,
    pub queue: String,
}

#[derive(Debug)]
pub struct EventRow {
    pub event: Event,
    pub lease_owner: String,
}

pub fn scan_event(row: &Row<'_>) -> rusqlite::Result<EventRow> {
    let source_kind: i64 = row.get(3)?;
    let action: i64 = row.get(4)?;
    let state: i64 = row.get(9)?;
    let ack_outcome: i64 = row.get(17)?;
    let event = Event {
        id: row.get(0)?,
        dedupe_key: row.get(1)?,
        source_id: row.get(2)?,
        source_kind: SourceKind::from_code(i32::try_from(source_kind).unwrap_or(0))
            .unwrap_or_default(),
        action: EventAction::from_code(i32::try_from(action).unwrap_or(0)).unwrap_or_default(),
        due_unix: row.get(5)?,
        occurrence_unix: row.get(6)?,
        label: row.get(7)?,
        payload_json: row.get(8)?,
        state: EventState::from_code(i32::try_from(state).unwrap_or(0)).unwrap_or_default(),
        lease_until_unix: row.get(11)?,
        attempts: i32::try_from(row.get::<_, i64>(12)?).unwrap_or(0),
        snooze_seq: row.get(13)?,
        detail: row.get(14)?,
        created_unix: row.get(15)?,
        acked_unix: row.get(16)?,
        ack_outcome: AckOutcome::from_code(i32::try_from(ack_outcome).unwrap_or(0))
            .unwrap_or_default(),
        task_type: row.get(18)?,
        queue: row.get(19)?,
    };
    Ok(EventRow {
        lease_owner: row.get(10)?,
        event,
    })
}

fn select_event(tx: &Transaction<'_>, id: &str) -> Result<EventRow, StoreError> {
    let sql = format!("SELECT {EVENT_COLS} FROM events WHERE id = ?");
    match tx.query_row(&sql, params![id], scan_event) {
        Ok(row) => Ok(row),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
        Err(error) => Err(error.into()),
    }
}

/// Inserts an outbox row, ignoring an existing identical dedupe identity.
pub fn insert_event(tx: &Transaction<'_>, mut input: EventInsert) -> Result<Event, StoreError> {
    if input.dedupe_key.is_empty() {
        return Err(StoreError::invalid("empty dedupe key"));
    }
    if input.queue.is_empty() {
        default_queue(input.source_kind).clone_into(&mut input.queue);
    }
    if input.task_type.is_empty() {
        input.task_type = default_task_type(input.source_kind, input.action);
    }
    let id = event_id(&input.dedupe_key);
    tx.execute(
        &format!(
            "INSERT OR IGNORE INTO events ({EVENT_COLS})
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ),
        params![
            id,
            input.dedupe_key,
            input.source_id,
            i64::from(input.source_kind.code()),
            i64::from(input.action.code()),
            input.due_unix,
            input.occurrence_unix,
            input.label,
            input.payload_json,
            i64::from(input.state.code()),
            "",
            0_i64,
            0_i64,
            input.snooze_seq,
            input.detail,
            input.created_unix,
            input.acked_unix,
            i64::from(input.ack_outcome.code()),
            input.task_type,
            input.queue,
        ],
    )?;
    Ok(select_event(tx, &id)?.event)
}

/// Cancels pending and leased FIRE deliveries for a source. A zero occurrence
/// means every occurrence of that source.
pub fn skip_fire_events(
    tx: &Transaction<'_>,
    source_id: &str,
    occurrence: i64,
    now_unix: i64,
    detail: &str,
) -> Result<(), StoreError> {
    let mut query = String::from(
        "UPDATE events SET state=?, lease_owner='', lease_until_unix=0,
            acked_unix=?, ack_outcome=?, detail=?
            WHERE source_id=? AND action=? AND state IN (?, ?)",
    );
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(i64::from(EventState::Skipped.code())),
        Box::new(now_unix),
        Box::new(i64::from(AckOutcome::Skipped.code())),
        Box::new(detail.to_owned()),
        Box::new(source_id.to_owned()),
        Box::new(i64::from(EventAction::Fire.code())),
        Box::new(i64::from(EventState::Pending.code())),
        Box::new(i64::from(EventState::Leased.code())),
    ];
    if occurrence > 0 {
        query.push_str(" AND occurrence_unix=?");
        values.push(Box::new(occurrence));
    }
    let params = values
        .iter()
        .map(|value| value.as_ref() as &dyn rusqlite::ToSql)
        .collect::<Vec<_>>();
    tx.execute(&query, params.as_slice())?;
    Ok(())
}

pub fn release_expired_leases(
    tx: &Transaction<'_>,
    now_unix: i64,
    max_attempts: i64,
) -> Result<i64, StoreError> {
    let released = tx.execute(
        "UPDATE events SET state=?, lease_owner='', lease_until_unix=0,
            detail=CASE WHEN detail='' THEN 'lease expired' ELSE detail END
            WHERE state=? AND lease_until_unix>0 AND lease_until_unix<=? AND attempts < ?",
        params![
            i64::from(EventState::Pending.code()),
            i64::from(EventState::Leased.code()),
            now_unix,
            max_attempts,
        ],
    )?;
    tx.execute(
        "UPDATE events SET state=?, lease_owner='', lease_until_unix=0,
            acked_unix=?, ack_outcome=?, detail='lease attempts exhausted'
            WHERE state=? AND lease_until_unix>0 AND lease_until_unix<=? AND attempts >= ?",
        params![
            i64::from(EventState::Failed.code()),
            now_unix,
            i64::from(AckOutcome::Failed.code()),
            i64::from(EventState::Leased.code()),
            now_unix,
            max_attempts,
        ],
    )?;
    Ok(i64::try_from(released).unwrap_or(0))
}

fn touch_consumer(
    tx: &Transaction<'_>,
    consumer_id: &str,
    now_unix: i64,
    ack: bool,
) -> Result<(), StoreError> {
    let last_ack = if ack { now_unix } else { 0 };
    tx.execute(
        "INSERT INTO consumers (consumer_id, last_seen_unix, last_ack_unix)
            VALUES (?,?,?)
            ON CONFLICT(consumer_id) DO UPDATE SET
                last_seen_unix=excluded.last_seen_unix,
                last_ack_unix=CASE WHEN excluded.last_ack_unix > consumers.last_ack_unix
                    THEN excluded.last_ack_unix ELSE consumers.last_ack_unix END",
        params![consumer_id, now_unix, last_ack],
    )?;
    Ok(())
}

fn validate_consumer(consumer_id: &str) -> Result<(), StoreError> {
    let valid = !consumer_id.is_empty()
        && consumer_id.len() <= 64
        && consumer_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid {
        return Err(StoreError::invalid(
            "consumer_id must be 1..64 chars of [A-Za-z0-9._-]",
        ));
    }
    Ok(())
}

impl Store {
    /// Leases up to `limit` due events for a consumer, optionally restricted to
    /// one queue. Expired leases from any consumer are released first.
    pub fn list_due_events(
        &self,
        consumer_id: &str,
        lease_seconds: i32,
        limit: i32,
        queue: &str,
        now_unix: i64,
    ) -> Result<Vec<Event>, StoreError> {
        validate_consumer(consumer_id)?;
        if !queue.is_empty() {
            validate_queue(queue)?;
        }
        let max_lease = self.config().max_lease;
        let lease = if lease_seconds == 0 {
            60
        } else if lease_seconds < 0 || i64::from(lease_seconds) > max_lease {
            return Err(StoreError::invalid(format!(
                "lease_seconds must be 1..{max_lease}"
            )));
        } else {
            i64::from(lease_seconds)
        };
        let limit = if limit == 0 {
            50
        } else if !(0..=200).contains(&limit) {
            return Err(StoreError::invalid("limit must be 1..200"));
        } else {
            limit
        };
        let max_attempts = self.config().max_attempts;
        let consumer = consumer_id.to_owned();
        let queue = queue.to_owned();
        self.with_tx(move |tx| {
            release_expired_leases(tx, now_unix, max_attempts)?;
            touch_consumer(tx, &consumer, now_unix, false)?;
            let mut query = String::from("SELECT id FROM events WHERE state = ? AND due_unix <= ?");
            let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(i64::from(EventState::Pending.code())),
                Box::new(now_unix),
            ];
            if !queue.is_empty() {
                query.push_str(" AND queue = ?");
                values.push(Box::new(queue));
            }
            query.push_str(" ORDER BY due_unix, id LIMIT ?");
            values.push(Box::new(i64::from(limit)));
            let params = values
                .iter()
                .map(|value| value.as_ref() as &dyn rusqlite::ToSql)
                .collect::<Vec<_>>();
            let mut statement = tx.prepare(&query)?;
            let ids = statement
                .query_map(params.as_slice(), |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            let mut events = Vec::with_capacity(ids.len());
            for id in ids {
                let changed = tx.execute(
                    "UPDATE events SET state=?, lease_owner=?, lease_until_unix=?, attempts=attempts+1
                        WHERE id=? AND state=?",
                    params![
                        i64::from(EventState::Leased.code()),
                        consumer,
                        now_unix.saturating_add(lease),
                        id,
                        i64::from(EventState::Pending.code()),
                    ],
                )?;
                if changed == 0 {
                    continue;
                }
                events.push(select_event(tx, &id)?.event);
            }
            Ok(events)
        })
    }

    /// Records a terminal outcome. Repeated acks are idempotent.
    pub fn ack_event(
        &self,
        event_id: &str,
        consumer_id: &str,
        outcome: AckOutcome,
        detail: &str,
        now_unix: i64,
    ) -> Result<Event, StoreError> {
        validate_event_id(event_id)?;
        validate_consumer(consumer_id)?;
        if !matches!(
            outcome,
            AckOutcome::Delivered | AckOutcome::Failed | AckOutcome::Skipped
        ) {
            return Err(StoreError::invalid(
                "ack outcome must be DELIVERED, FAILED or SKIPPED",
            ));
        }
        validate_detail(detail)?;
        let event_id = event_id.to_owned();
        let consumer = consumer_id.to_owned();
        let detail = detail.to_owned();
        self.with_tx(move |tx| {
            let row = select_event(tx, &event_id)?;
            let event = row.event;
            if event.state.is_terminal() {
                return Ok(event);
            }
            if event.state != EventState::Leased {
                return Err(StoreError::precondition("event is not leased"));
            }
            if row.lease_owner != consumer {
                return Err(StoreError::precondition(
                    "event is leased by another consumer",
                ));
            }
            if event.lease_until_unix <= now_unix {
                return Err(StoreError::precondition("lease has expired"));
            }
            touch_consumer(tx, &consumer, now_unix, true)?;
            let detail = if detail.is_empty() {
                event.detail
            } else {
                detail
            };
            let state = outcome.terminal_state();
            tx.execute(
                "UPDATE events SET state=?, acked_unix=?, ack_outcome=?,
                    lease_owner='', lease_until_unix=0, detail=? WHERE id=?",
                params![
                    i64::from(state.code()),
                    now_unix,
                    i64::from(outcome.code()),
                    detail,
                    event_id,
                ],
            )?;
            Ok(select_event(tx, &event_id)?.event)
        })
    }
}

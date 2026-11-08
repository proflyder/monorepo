//! Schedule CRUD, recurrence advancement and ringing-window control.

use rusqlite::{Row, Transaction, params};

use crate::domain::{
    ErrorKind, Repeat, Schedule, ScheduleKind, WallTime, hash::sha1_hex, validate_id,
    validate_idempotency_key, validate_label, validate_payload, validate_reminder_repeat,
    validate_schedule_kind,
};
use crate::domain::{event as keys, resource_id};

use super::event::{EventInsert, insert_event, skip_fire_events};
use super::{Store, StoreError, bool_to_int, kind_code, weekdays_from_text, weekdays_text};
use crate::domain::Event;

pub const SCHEDULE_COLS: &str =
    "id, kind, label, timezone, at_hour, at_minute, at_second, once_date,
	repeat_kind, weekdays, next_due_unix, enabled, payload_json, revision,
	created_unix, updated_unix, deleted_unix, last_fired_unix, snooze_until_unix,
	snooze_count, active_occurrence_unix, active_until_unix";

pub const DETAIL_RING_WINDOW_EXPIRED: &str = "ring_window_expired";
pub const DETAIL_RULE_CHANGED: &str = "rule_changed";

pub fn scan_schedule(row: &Row<'_>) -> rusqlite::Result<Schedule> {
    let kind: i64 = row.get(1)?;
    let repeat: i64 = row.get(8)?;
    let weekdays: String = row.get(9)?;
    let enabled: i64 = row.get(11)?;
    Ok(Schedule {
        id: row.get(0)?,
        kind: ScheduleKind::from_code(i32::try_from(kind).unwrap_or(0)).unwrap_or_default(),
        label: row.get(2)?,
        timezone: row.get(3)?,
        at: WallTime {
            hour: i32::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
            minute: i32::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
            second: i32::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
        },
        once_date: row.get(7)?,
        repeat: Repeat::from_code(i32::try_from(repeat).unwrap_or(0)).unwrap_or_default(),
        weekdays: weekdays_from_text(&weekdays),
        next_due_unix: row.get(10)?,
        enabled: enabled != 0,
        payload_json: row.get(12)?,
        revision: row.get(13)?,
        created_unix: row.get(14)?,
        updated_unix: row.get(15)?,
        deleted_unix: row.get(16)?,
        last_fired_unix: row.get(17)?,
        snooze_until_unix: row.get(18)?,
        snooze_count: row.get(19)?,
        active_occurrence_unix: row.get(20)?,
        active_until_unix: row.get(21)?,
    })
}

fn get_schedule_tx(tx: &Transaction<'_>, id: &str) -> Result<Schedule, StoreError> {
    let sql = format!("SELECT {SCHEDULE_COLS} FROM schedules WHERE id = ? AND deleted_unix = 0");
    match tx.query_row(&sql, params![id], scan_schedule) {
        Ok(schedule) => Ok(schedule),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
        Err(error) => Err(error.into()),
    }
}

fn get_schedule_any_tx(tx: &Transaction<'_>, id: &str) -> Result<(Schedule, i64), StoreError> {
    let sql = format!("SELECT {SCHEDULE_COLS} FROM schedules WHERE id = ?");
    match tx.query_row(&sql, params![id], |row| {
        let schedule = scan_schedule(row)?;
        let deleted: i64 = row.get(16)?;
        Ok((schedule, deleted))
    }) {
        Ok(value) => Ok(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
        Err(error) => Err(error.into()),
    }
}

/// Validates and normalizes client input, never trusting server-derived
/// fields present in the request.
fn prepare_schedule(store: &Store, input: &Schedule) -> Result<Schedule, StoreError> {
    let mut out = Schedule {
        id: input.id.trim().to_owned(),
        label: input.label.clone(),
        kind: input.kind,
        timezone: input.timezone.trim().to_owned(),
        once_date: input.once_date.trim().to_owned(),
        repeat: input.repeat,
        weekdays: input.weekdays.clone(),
        enabled: input.enabled,
        payload_json: input.payload_json.clone(),
        at: input.at,
        ..Schedule::default()
    };
    validate_label(&out.label)?;
    validate_schedule_kind(out.kind)?;
    if out.timezone.is_empty() {
        out.timezone.clone_from(&store.config().default_timezone);
    }
    validate_reminder_repeat(out.kind, out.repeat)?;
    if out.payload_json.is_empty() {
        "{}".clone_into(&mut out.payload_json);
    }
    validate_payload(&out.payload_json, store.config().max_payload_bytes)?;
    out.rule()
        .validate()
        .map_err(|error| StoreError::Domain(crate::domain::DomainError::invalid_rule(&error)))?;
    Ok(out)
}

fn next_occurrence(schedule: &Schedule, now_unix: i64) -> Result<i64, StoreError> {
    match schedule.rule().next(now_unix) {
        Ok(Some(next)) => Ok(next),
        Ok(None) => Ok(0),
        Err(error) => Err(crate::domain::DomainError::invalid_rule(&error).into()),
    }
}

/// Scenario-owned rows are managed only through `ScenarioService`.
fn reject_scenario_schedule(schedule: &Schedule) -> Result<(), StoreError> {
    if schedule.kind != ScheduleKind::Scenario {
        return Ok(());
    }
    let owner = serde_json::from_str::<serde_json::Value>(&schedule.payload_json)
        .ok()
        .and_then(|payload| payload.get("scenario_id")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| schedule.id.clone());
    Err(StoreError::precondition(format!(
        "managed by scenario {owner}"
    )))
}

impl Store {
    /// Creates a durable schedule. A non-empty idempotency key replays the
    /// original resource when the canonical request matches.
    pub fn create_schedule(
        &self,
        input: &Schedule,
        idempotency_key: &str,
        now_unix: i64,
    ) -> Result<Schedule, StoreError> {
        let mut prepared = prepare_schedule(self, input)?;
        if prepared.kind == ScheduleKind::Scenario {
            return Err(StoreError::invalid(
                "scenario schedules are created through ScenarioService",
            ));
        }
        validate_idempotency_key(idempotency_key)?;
        prepared.id = resource_id(now_unix.saturating_mul(1_000));
        prepared.revision = 1;
        prepared.created_unix = now_unix;
        prepared.updated_unix = now_unix;
        prepared.next_due_unix = 0;
        if prepared.enabled {
            let next = next_occurrence(&prepared, now_unix)?;
            if next == 0 {
                return Err(StoreError::invalid(
                    "one-shot date is in the past or does not exist",
                ));
            }
            prepared.next_due_unix = next;
        }
        let hash = sha1_hex(&prepared.canonical());
        let key = idempotency_key.to_owned();
        let row = prepared;
        self.with_tx(move |tx| {
            if !key.is_empty()
                && let Some((kind, resource_id, existing_hash)) = idempotency_lookup(tx, &key)?
            {
                    if kind != "schedule" || existing_hash != hash {
                        return Err(StoreError::domain(
                            ErrorKind::Idempotency,
                            "idempotency key reused with a different request",
                        ));
                    }
                    let schedule = get_schedule_tx(tx, &resource_id).map_err(|error| {
                        if matches!(error, StoreError::NotFound) {
                            StoreError::domain(
                                ErrorKind::Idempotency,
                                "idempotent schedule no longer exists",
                            )
                        } else {
                            error
                        }
                    })?;
                    return Ok(schedule);
            }
            let sql = format!("INSERT INTO schedules ({SCHEDULE_COLS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)");
            tx.execute(
                &sql,
                params![
                    row.id,
                    kind_code(row.kind),
                    row.label,
                    row.timezone,
                    i64::from(row.at.hour),
                    i64::from(row.at.minute),
                    i64::from(row.at.second),
                    row.once_date,
                    i64::from(row.repeat.code()),
                    weekdays_text(&row.weekdays),
                    row.next_due_unix,
                    bool_to_int(row.enabled),
                    row.payload_json,
                    row.revision,
                    row.created_unix,
                    row.updated_unix,
                    0_i64,
                    row.last_fired_unix,
                    row.snooze_until_unix,
                    row.snooze_count,
                    row.active_occurrence_unix,
                    row.active_until_unix,
                ],
            )?;
            if !key.is_empty() {
                tx.execute(
                    r"INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)",
                    params![key, "schedule", row.id, hash, now_unix],
                )
                .map_err(|_| {
                    StoreError::domain(ErrorKind::Idempotency, "idempotency key already exists")
                })?;
            }
            Ok(row)
        })
    }

    /// Returns a live schedule.
    pub fn get_schedule(&self, id: &str) -> Result<Schedule, StoreError> {
        validate_id(id)?;
        let id = id.to_owned();
        self.read(move |connection| {
            let sql =
                format!("SELECT {SCHEDULE_COLS} FROM schedules WHERE id = ? AND deleted_unix = 0");
            match connection.query_row(&sql, params![id], scan_schedule) {
                Ok(schedule) => Ok(schedule),
                Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
                Err(error) => Err(error.into()),
            }
        })
    }

    /// Live schedules ordered by creation time.
    pub fn list_schedules(
        &self,
        include_disabled: bool,
        kind: ScheduleKind,
    ) -> Result<Vec<Schedule>, StoreError> {
        self.read(move |connection| {
            let mut query = format!("SELECT {SCHEDULE_COLS} FROM schedules WHERE deleted_unix = 0");
            let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if !include_disabled {
                query.push_str(" AND enabled = 1");
            }
            if kind == ScheduleKind::Unspecified {
                // Scenario rows are internal trigger materialization. They are
                // returned only by an explicit kind=SCENARIO query so alarm and
                // reminder clients never render them as schedules.
                query.push_str(" AND kind != ?");
                values.push(Box::new(kind_code(ScheduleKind::Scenario)));
            } else {
                query.push_str(" AND kind = ?");
                values.push(Box::new(kind_code(kind)));
            }
            query.push_str(" ORDER BY created_unix, id");
            let mut statement = connection.prepare(&query)?;
            let params = values
                .iter()
                .map(|value| value.as_ref() as &dyn rusqlite::ToSql)
                .collect::<Vec<_>>();
            let rows = statement.query_map(params.as_slice(), scan_schedule)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        })
    }

    /// Replaces a schedule rule; `expected_revision = 0` disables the
    /// optimistic concurrency check.
    pub fn update_schedule(
        &self,
        input: &Schedule,
        expected_revision: i64,
        now_unix: i64,
    ) -> Result<Schedule, StoreError> {
        validate_id(&input.id)?;
        let mut prepared = prepare_schedule(self, input)?;
        let ring_window = self.config().ring_window;
        self.with_tx(move |tx| {
            let existing = get_schedule_tx(tx, &prepared.id)?;
            reject_scenario_schedule(&existing)?;
            if prepared.kind == ScheduleKind::Scenario {
                return Err(StoreError::invalid(
                    "scenario schedules are created through ScenarioService",
                ));
            }
            if expected_revision != 0 && expected_revision != existing.revision {
                return Err(StoreError::domain(
                    ErrorKind::Conflict,
                    format!(
                        "current revision {}, expected {}",
                        existing.revision, expected_revision
                    ),
                ));
            }
            prepared.revision = existing.revision + 1;
            prepared.created_unix = existing.created_unix;
            prepared.updated_unix = now_unix;
            prepared.last_fired_unix = existing.last_fired_unix;
            prepared.next_due_unix = 0;
            if prepared.enabled {
                let next = next_occurrence(&prepared, now_unix)?;
                if next == 0 {
                    return Err(StoreError::invalid(
                        "one-shot date is in the past or does not exist",
                    ));
                }
                prepared.next_due_unix = next;
            }
            if let Some(occurrence) = existing.occurrence_to_close(now_unix) {
                close_occurrence(
                    tx,
                    &existing,
                    occurrence.unix(),
                    DETAIL_RULE_CHANGED,
                    now_unix,
                    ring_window,
                )?;
                prepared.revision += 1;
            }
            prepared.snooze_until_unix = 0;
            prepared.snooze_count = 0;
            prepared.active_occurrence_unix = 0;
            prepared.active_until_unix = 0;
            tx.execute(
                "UPDATE schedules SET
                    kind=?, label=?, timezone=?, at_hour=?, at_minute=?, at_second=?, once_date=?,
                    repeat_kind=?, weekdays=?, next_due_unix=?, enabled=?, payload_json=?,
                    revision=?, updated_unix=?, last_fired_unix=?, snooze_until_unix=?,
                    snooze_count=?, active_occurrence_unix=?, active_until_unix=?
                    WHERE id=? AND deleted_unix=0",
                params![
                    kind_code(prepared.kind),
                    prepared.label,
                    prepared.timezone,
                    i64::from(prepared.at.hour),
                    i64::from(prepared.at.minute),
                    i64::from(prepared.at.second),
                    prepared.once_date,
                    i64::from(prepared.repeat.code()),
                    weekdays_text(&prepared.weekdays),
                    prepared.next_due_unix,
                    bool_to_int(prepared.enabled),
                    prepared.payload_json,
                    prepared.revision,
                    prepared.updated_unix,
                    prepared.last_fired_unix,
                    prepared.snooze_until_unix,
                    prepared.snooze_count,
                    prepared.active_occurrence_unix,
                    prepared.active_until_unix,
                    prepared.id,
                ],
            )?;
            Ok(prepared)
        })
    }

    /// Toggles a schedule. Enabling recomputes the next occurrence after now.
    pub fn set_schedule_enabled(
        &self,
        id: &str,
        enabled: bool,
        now_unix: i64,
    ) -> Result<Schedule, StoreError> {
        let id = id.to_owned();
        let ring_window = self.config().ring_window;
        self.with_tx(move |tx| {
            let mut current = get_schedule_tx(tx, &id)?;
            reject_scenario_schedule(&current)?;
            if current.enabled == enabled {
                return Ok(current);
            }
            let mut next = 0;
            if enabled {
                next = next_occurrence(&current, now_unix)?;
                if next == 0 {
                    return Err(StoreError::invalid(
                        "one-shot date is in the past or does not exist",
                    ));
                }
            }
            current.enabled = enabled;
            current.next_due_unix = next;
            current.revision += 1;
            current.updated_unix = now_unix;
            if enabled {
                if let Some(occurrence) = current.occurrence_to_close(now_unix) {
                    close_occurrence(
                        tx,
                        &current,
                        occurrence.unix(),
                        DETAIL_RULE_CHANGED,
                        now_unix,
                        ring_window,
                    )?;
                    current.revision += 1;
                }
                current.snooze_until_unix = 0;
                current.snooze_count = 0;
                current.active_occurrence_unix = 0;
                current.active_until_unix = 0;
            }
            tx.execute(
                "UPDATE schedules SET enabled=?, next_due_unix=?, revision=?, updated_unix=?,
                    snooze_until_unix=?, snooze_count=?, active_occurrence_unix=?, active_until_unix=? WHERE id=?",
                params![
                    bool_to_int(current.enabled),
                    current.next_due_unix,
                    current.revision,
                    current.updated_unix,
                    current.snooze_until_unix,
                    current.snooze_count,
                    current.active_occurrence_unix,
                    current.active_until_unix,
                    current.id,
                ],
            )?;
            Ok(current)
        })
    }

    /// Soft-deletes a schedule, stops pending triggers and queues a STOP when
    /// an occurrence is active.
    pub fn delete_schedule(&self, id: &str, now_unix: i64) -> Result<bool, StoreError> {
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let (schedule, deleted) = get_schedule_any_tx(tx, &id)?;
            if deleted != 0 {
                return Ok(false);
            }
            reject_scenario_schedule(&schedule)?;
            if let Some(occurrence) = schedule.active_occurrence(now_unix) {
                insert_event(
                    tx,
                    EventInsert {
                        dedupe_key: keys::schedule_control_stop_key(
                            &schedule.id,
                            occurrence.unix(),
                            0,
                        ),
                        source_id: schedule.id.clone(),
                        source_kind: schedule.kind.source_kind(),
                        action: crate::domain::EventAction::Stop,
                        due_unix: now_unix,
                        occurrence_unix: occurrence.unix(),
                        label: schedule.label.clone(),
                        payload_json: schedule.payload_json.clone(),
                        state: crate::domain::EventState::Pending,
                        detail: "deleted".to_owned(),
                        created_unix: now_unix,
                        ..EventInsert::default()
                    },
                )?;
            }
            skip_fire_events(tx, &schedule.id, 0, now_unix, "deleted")?;
            tx.execute(
                "UPDATE schedules SET deleted_unix=?, enabled=0, next_due_unix=0,
                    revision=revision+1, updated_unix=?, active_occurrence_unix=0, active_until_unix=0,
                    snooze_until_unix=0 WHERE id=?",
                params![now_unix, now_unix, schedule.id],
            )?;
            Ok(true)
        })
    }

    /// Queues an immediate STOP and a delayed FIRE occurrence.
    pub fn snooze_schedule(
        &self,
        id: &str,
        minutes: i32,
        now_unix: i64,
    ) -> Result<(Schedule, Event), StoreError> {
        if !(0..=1440).contains(&minutes) {
            return Err(StoreError::invalid("snooze minutes must be 0..1440"));
        }
        let minutes = if minutes == 0 {
            self.config().default_snooze_minutes
        } else {
            minutes
        };
        let ring_window = self.config().ring_window;
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let mut schedule = get_schedule_tx(tx, &id)?;
            reject_scenario_schedule(&schedule)?;
            let Some(occurrence) = schedule.active_occurrence(now_unix) else {
                return Err(StoreError::precondition("schedule is not ringing"));
            };
            let occurrence = occurrence.unix();
            let max_seq: Option<i64> = tx.query_row(
                "SELECT MAX(snooze_seq) FROM events WHERE source_id = ? AND occurrence_unix = ?",
                params![schedule.id, occurrence],
                |row| row.get(0),
            )?;
            let mut seq = i32::try_from(schedule.snooze_count + 1).unwrap_or(i32::MAX);
            if let Some(max_seq) = max_seq
                && max_seq >= i64::from(seq)
            {
                seq = i32::try_from(max_seq + 1).unwrap_or(i32::MAX);
            }
            let snooze_due = now_unix.saturating_add(i64::from(minutes) * 60);
            insert_event(
                tx,
                EventInsert {
                    dedupe_key: keys::schedule_control_stop_key(&schedule.id, occurrence, i64::from(seq)),
                    source_id: schedule.id.clone(),
                    source_kind: schedule.kind.source_kind(),
                    action: crate::domain::EventAction::Stop,
                    due_unix: now_unix,
                    occurrence_unix: occurrence,
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: crate::domain::EventState::Pending,
                    detail: format!("snoozed for {minutes} minutes"),
                    created_unix: now_unix,
                    snooze_seq: i64::from(seq),
                    ..EventInsert::default()
                },
            )?;
            skip_fire_events(tx, &schedule.id, 0, now_unix, "snoozed")?;
            let fire = insert_event(
                tx,
                EventInsert {
                    dedupe_key: keys::schedule_trigger_key(
                        schedule.kind.source_kind(),
                        &schedule.id,
                        occurrence,
                        i64::from(seq),
                    ),
                    source_id: schedule.id.clone(),
                    source_kind: schedule.kind.source_kind(),
                    action: crate::domain::EventAction::Fire,
                    due_unix: snooze_due,
                    occurrence_unix: occurrence,
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: crate::domain::EventState::Pending,
                    created_unix: now_unix,
                    snooze_seq: i64::from(seq),
                    ..EventInsert::default()
                },
            )?;
            schedule.snooze_until_unix = snooze_due;
            schedule.snooze_count = i64::from(seq);
            schedule.active_occurrence_unix = occurrence;
            schedule.active_until_unix = snooze_due.saturating_add(ring_window);
            schedule.revision += 1;
            schedule.updated_unix = now_unix;
            tx.execute(
                "UPDATE schedules SET snooze_until_unix=?, snooze_count=?,
                    active_occurrence_unix=?, active_until_unix=?, revision=?, updated_unix=? WHERE id=?",
                params![
                    schedule.snooze_until_unix,
                    schedule.snooze_count,
                    schedule.active_occurrence_unix,
                    schedule.active_until_unix,
                    schedule.revision,
                    schedule.updated_unix,
                    schedule.id,
                ],
            )?;
            Ok((schedule, fire))
        })
    }

    /// Queues an immediate STOP, cancels pending triggers and clears the
    /// active window without touching recurrence.
    pub fn stop_schedule(&self, id: &str, now_unix: i64) -> Result<Schedule, StoreError> {
        let id = id.to_owned();
        let ring_window = self.config().ring_window;
        self.with_tx(move |tx| {
            let mut schedule = get_schedule_tx(tx, &id)?;
            reject_scenario_schedule(&schedule)?;
            let Some(occurrence) = schedule.active_occurrence(now_unix) else {
                let prior: Option<i64> = tx
                    .query_row(
                        "SELECT occurrence_unix FROM events
                            WHERE source_id=? AND action=? AND detail IN (?, ?, ?) AND due_unix>=?
                            ORDER BY created_unix DESC LIMIT 1",
                        params![
                            schedule.id,
                            i64::from(crate::domain::EventAction::Stop.code()),
                            "stopped",
                            DETAIL_RING_WINDOW_EXPIRED,
                            DETAIL_RULE_CHANGED,
                            now_unix.saturating_sub(ring_window),
                        ],
                        |row| row.get(0),
                    )
                    .ok();
                if prior.is_some() {
                    return Ok(schedule);
                }
                return Err(StoreError::precondition("schedule is not ringing"));
            };
            insert_event(
                tx,
                EventInsert {
                    dedupe_key: keys::schedule_control_stop_key(&schedule.id, occurrence.unix(), 0),
                    source_id: schedule.id.clone(),
                    source_kind: schedule.kind.source_kind(),
                    action: crate::domain::EventAction::Stop,
                    due_unix: now_unix,
                    occurrence_unix: occurrence.unix(),
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: crate::domain::EventState::Pending,
                    detail: "stopped".to_owned(),
                    created_unix: now_unix,
                    ..EventInsert::default()
                },
            )?;
            skip_fire_events(tx, &schedule.id, 0, now_unix, "stopped")?;
            schedule.active_occurrence_unix = 0;
            schedule.active_until_unix = 0;
            schedule.snooze_until_unix = 0;
            schedule.revision += 1;
            schedule.updated_unix = now_unix;
            tx.execute(
                "UPDATE schedules SET active_occurrence_unix=0, active_until_unix=0,
                    snooze_until_unix=0, revision=?, updated_unix=? WHERE id=?",
                params![schedule.revision, schedule.updated_unix, schedule.id],
            )?;
            Ok(schedule)
        })
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

pub fn close_occurrence(
    tx: &Transaction<'_>,
    schedule: &Schedule,
    occurrence: i64,
    detail: &str,
    now_unix: i64,
    _ring_window: i64,
) -> Result<bool, StoreError> {
    if schedule.id.is_empty() {
        return Ok(false);
    }
    if occurrence > 0 {
        skip_fire_events(tx, &schedule.id, occurrence, now_unix, detail)?;
        if schedule.kind == ScheduleKind::Alarm {
            insert_event(
                tx,
                EventInsert {
                    dedupe_key: keys::expiry_stop_key(
                        schedule.kind.source_kind(),
                        &schedule.id,
                        occurrence,
                    ),
                    source_id: schedule.id.clone(),
                    source_kind: schedule.kind.source_kind(),
                    action: crate::domain::EventAction::Stop,
                    due_unix: now_unix,
                    occurrence_unix: occurrence,
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: crate::domain::EventState::Pending,
                    detail: detail.to_owned(),
                    created_unix: now_unix,
                    ..EventInsert::default()
                },
            )?;
        }
    }
    tx.execute(
        "UPDATE schedules SET active_occurrence_unix=0,
            active_until_unix=0, snooze_until_unix=0, revision=revision+1, updated_unix=?
            WHERE id=?",
        params![now_unix, schedule.id],
    )?;
    Ok(true)
}

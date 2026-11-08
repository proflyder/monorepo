//! Scenario definitions, revision control and time-trigger materialization.
//!
//! A time trigger is materialized as a real `schedules` row with
//! `kind=SCHEDULE_KIND_SCENARIO` and an opaque payload
//! `{"scenario_id":"…","trigger_index":n}`. Recurrence, DST handling, missed
//! grace, dedupe identities and the outbox are therefore shared with alarms.
//! The scheduler only stores/validates conditions and steps; the future
//! `ha-scenario` executor evaluates and runs them.

use rusqlite::{Connection, Row, Transaction, params};

use crate::domain::{
    ErrorKind, Event, EventAction, EventState, Scenario, ScenarioDefinition, SourceKind,
    canonical_scenario, default_queue, default_task_type, event_id, hash::sha1_hex, random_id,
    resource_id, scenario_manual_key, schedule_control_stop_key, validate_id,
    validate_idempotency_key, weekdays_text,
};

use super::event::{EventInsert, insert_event};
use super::schedule::{DETAIL_RULE_CHANGED, SCHEDULE_COLS, close_occurrence, scan_schedule};
use super::{Store, StoreError, bool_to_int, kind_code};

pub const SCENARIO_COLS: &str = "id, body_json, enabled, revision, created_unix, updated_unix";

pub const DETAIL_SCENARIO_DELETED: &str = "deleted";

fn scan_scenario(row: &Row<'_>) -> rusqlite::Result<Scenario> {
    let id: String = row.get(0)?;
    let body: String = row.get(1)?;
    let enabled: i64 = row.get(2)?;
    let revision: i64 = row.get(3)?;
    let created_unix: i64 = row.get(4)?;
    let updated_unix: i64 = row.get(5)?;
    let definition: ScenarioDefinition = serde_json::from_str(&body).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Scenario::from_definition(
        id,
        definition,
        enabled != 0,
        revision,
        created_unix,
        updated_unix,
    ))
}

fn get_scenario_tx(tx: &Transaction<'_>, id: &str) -> Result<Scenario, StoreError> {
    let sql = format!("SELECT {SCENARIO_COLS} FROM scenarios WHERE id = ?");
    match tx.query_row(&sql, params![id], scan_scenario) {
        Ok(mut scenario) => {
            scenario.next_due_unix = next_due(tx, &scenario.id)?;
            Ok(scenario)
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
        Err(error) => Err(error.into()),
    }
}

/// Validates and copies client input, never trusting server-derived fields.
fn prepare_scenario(store: &Store, input: &Scenario) -> Result<Scenario, StoreError> {
    let out = Scenario {
        id: input.id.clone(),
        label: input.label.clone(),
        description: input.description.clone(),
        enabled: input.enabled,
        triggers: input.triggers.clone(),
        conditions: input.conditions.clone(),
        steps: input.steps.clone(),
        continue_on_error: input.continue_on_error,
        ..Scenario::default()
    };
    Scenario::validate(&out.definition(), store.config().max_payload_bytes)?;
    Ok(out)
}

fn next_due_of(scenario: &Scenario, now_unix: i64) -> Result<i64, StoreError> {
    if !scenario.enabled {
        return Ok(0);
    }
    let mut next = 0;
    for (_, time) in scenario.time_triggers() {
        match time.rule().next(now_unix) {
            Ok(Some(occurrence)) => {
                next = if next == 0 {
                    occurrence
                } else {
                    next.min(occurrence)
                };
            }
            Ok(None) => {
                return Err(StoreError::invalid(
                    "one-shot date is in the past or does not exist",
                ));
            }
            Err(error) => {
                return Err(crate::domain::DomainError::invalid_rule(&error).into());
            }
        }
    }
    Ok(next)
}

/// Earliest next occurrence of the materialized scenario schedules.
fn next_due(connection: &Connection, scenario_id: &str) -> Result<i64, StoreError> {
    let sql = "SELECT payload_json, next_due_unix FROM schedules
        WHERE kind = ? AND deleted_unix = 0 AND enabled = 1 AND next_due_unix > 0";
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(
        params![kind_code(crate::domain::ScheduleKind::Scenario)],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let mut next = 0;
    for row in rows {
        let (payload, due) = row?;
        if scenario_id_of(&payload).as_deref() == Some(scenario_id) {
            next = if next == 0 { due } else { next.min(due) };
        }
    }
    Ok(next)
}

fn scenario_id_of(payload_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload_json).ok()?;
    value.get("scenario_id")?.as_str().map(str::to_owned)
}

fn load_scenario_schedules(
    tx: &Transaction<'_>,
    scenario_id: &str,
) -> Result<Vec<crate::domain::Schedule>, StoreError> {
    let sql = format!("SELECT {SCHEDULE_COLS} FROM schedules WHERE kind = ? AND deleted_unix = 0");
    let mut statement = tx.prepare(&sql)?;
    let schedules = statement
        .query_map(
            params![kind_code(crate::domain::ScheduleKind::Scenario)],
            scan_schedule,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(schedules
        .into_iter()
        .filter(|schedule| scenario_id_of(&schedule.payload_json).as_deref() == Some(scenario_id))
        .collect())
}

fn remove_scenario_schedules(
    tx: &Transaction<'_>,
    scenario: &Scenario,
    detail: &str,
    now_unix: i64,
    ring_window: i64,
) -> Result<(), StoreError> {
    for schedule in load_scenario_schedules(tx, &scenario.id)? {
        if let Some(occurrence) = schedule.occurrence_to_close(now_unix) {
            // A scenario has no audio to silence, so `close_occurrence` only
            // cancels the pending FIRE. Queue an explicit STOP so a running
            // executor can abort the occurrence, matching the alarm
            // `rule_changed` behaviour.
            insert_event(
                tx,
                EventInsert {
                    dedupe_key: schedule_control_stop_key(&schedule.id, occurrence.unix(), 0),
                    source_id: schedule.id.clone(),
                    source_kind: SourceKind::Scenario,
                    action: EventAction::Stop,
                    due_unix: now_unix,
                    occurrence_unix: occurrence.unix(),
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: EventState::Pending,
                    detail: detail.to_owned(),
                    created_unix: now_unix,
                    ..EventInsert::default()
                },
            )?;
            close_occurrence(
                tx,
                &schedule,
                occurrence.unix(),
                DETAIL_RULE_CHANGED,
                now_unix,
                ring_window,
            )?;
        }
        super::event::skip_fire_events(tx, &schedule.id, 0, now_unix, detail)?;
        tx.execute("DELETE FROM schedules WHERE id = ?", params![schedule.id])?;
    }
    Ok(())
}

fn schedule_id(scenario_id: &str, trigger_index: usize) -> String {
    event_id(&format!("scenario-schedule:{scenario_id}:{trigger_index}"))
}

fn materialize_schedules(
    tx: &Transaction<'_>,
    scenario: &Scenario,
    now_unix: i64,
    ring_window: i64,
    detail: &str,
) -> Result<i64, StoreError> {
    remove_scenario_schedules(tx, scenario, detail, now_unix, ring_window)?;
    let mut next_due = 0;
    for (index, time) in scenario.time_triggers() {
        let next = if scenario.enabled {
            match time.rule().next(now_unix) {
                Ok(Some(next)) => next,
                Ok(None) => {
                    return Err(StoreError::invalid(
                        "one-shot date is in the past or does not exist",
                    ));
                }
                Err(error) => {
                    return Err(crate::domain::DomainError::invalid_rule(&error).into());
                }
            }
        } else {
            0
        };
        if scenario.enabled {
            next_due = if next_due == 0 {
                next
            } else {
                next_due.min(next)
            };
        }
        let payload = serde_json::json!({
            "scenario_id": scenario.id,
            "trigger_index": index,
        })
        .to_string();
        let id = schedule_id(&scenario.id, index);
        tx.execute(
            &format!("INSERT INTO schedules ({SCHEDULE_COLS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
            params![
                id,
                kind_code(crate::domain::ScheduleKind::Scenario),
                scenario.label,
                time.timezone,
                i64::from(time.at.hour),
                i64::from(time.at.minute),
                i64::from(time.at.second),
                time.once_date,
                i64::from(time.repeat.code()),
                weekdays_text(&time.weekdays),
                next,
                bool_to_int(scenario.enabled),
                payload,
                1_i64,
                now_unix,
                now_unix,
                0_i64,
                0_i64,
                0_i64,
                0_i64,
                0_i64,
                0_i64,
            ],
        )?;
    }
    Ok(next_due)
}

fn insert_scenario(
    tx: &Transaction<'_>,
    scenario: &Scenario,
    body_json: &str,
) -> Result<(), StoreError> {
    tx.execute(
        &format!("INSERT INTO scenarios ({SCENARIO_COLS}) VALUES (?,?,?,?,?,?)"),
        params![
            scenario.id,
            body_json,
            bool_to_int(scenario.enabled),
            scenario.revision,
            scenario.created_unix,
            scenario.updated_unix,
        ],
    )?;
    Ok(())
}

fn update_scenario_row(tx: &Transaction<'_>, scenario: &Scenario) -> Result<(), StoreError> {
    let body = scenario.body_json()?;
    tx.execute(
        "UPDATE scenarios SET body_json=?, enabled=?, revision=?, updated_unix=? WHERE id=?",
        params![
            body,
            bool_to_int(scenario.enabled),
            scenario.revision,
            scenario.updated_unix,
            scenario.id,
        ],
    )?;
    Ok(())
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
    /// Creates a scenario and materializes its time triggers. A non-empty
    /// idempotency key replays the original definition when the canonical
    /// request matches.
    pub fn create_scenario(
        &self,
        input: &Scenario,
        idempotency_key: &str,
        now_unix: i64,
    ) -> Result<Scenario, StoreError> {
        let mut prepared = prepare_scenario(self, input)?;
        validate_idempotency_key(idempotency_key)?;
        let next = next_due_of(&prepared, now_unix)?;
        prepared.id = resource_id(now_unix.saturating_mul(1_000));
        prepared.revision = 1;
        prepared.created_unix = now_unix;
        prepared.updated_unix = now_unix;
        prepared.next_due_unix = next;
        let body = prepared.body_json()?;
        let hash = sha1_hex(&canonical_scenario(&body, prepared.enabled));
        let key = idempotency_key.to_owned();
        let ring_window = self.config().ring_window;
        let row = prepared;
        self.with_tx(move |tx| {
            if !key.is_empty()
                && let Some((kind, resource_id, existing_hash)) = idempotency_lookup(tx, &key)?
            {
                if kind != "scenario" || existing_hash != hash {
                    return Err(StoreError::domain(
                        ErrorKind::Idempotency,
                        "idempotency key reused with a different request",
                    ));
                }
                return get_scenario_tx(tx, &resource_id).map_err(|error| {
                    if matches!(error, StoreError::NotFound) {
                        StoreError::domain(
                            ErrorKind::Idempotency,
                            "idempotent scenario no longer exists",
                        )
                    } else {
                        error
                    }
                });
            }
            insert_scenario(tx, &row, &body)?;
            let next =
                materialize_schedules(tx, &row, now_unix, ring_window, DETAIL_RULE_CHANGED)?;
            if !key.is_empty() {
                tx.execute(
                    r"INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)",
                    params![key, "scenario", row.id, hash, now_unix],
                )
                .map_err(|_| {
                    StoreError::domain(ErrorKind::Idempotency, "idempotency key already exists")
                })?;
            }
            let mut created = row;
            created.next_due_unix = next;
            Ok(created)
        })
    }

    /// Returns a scenario with its computed next occurrence.
    pub fn get_scenario(&self, id: &str) -> Result<Scenario, StoreError> {
        validate_id(id)?;
        let id = id.to_owned();
        self.read(move |connection| {
            let sql = format!("SELECT {SCENARIO_COLS} FROM scenarios WHERE id = ?");
            match connection.query_row(&sql, params![id], scan_scenario) {
                Ok(mut scenario) => {
                    scenario.next_due_unix = next_due(connection, &scenario.id)?;
                    Ok(scenario)
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::NotFound),
                Err(error) => Err(error.into()),
            }
        })
    }

    /// Scenarios ordered by creation time.
    pub fn list_scenarios(&self, include_disabled: bool) -> Result<Vec<Scenario>, StoreError> {
        self.read(move |connection| {
            let mut query = format!("SELECT {SCENARIO_COLS} FROM scenarios");
            if !include_disabled {
                query.push_str(" WHERE enabled = 1");
            }
            query.push_str(" ORDER BY created_unix, id");
            let mut statement = connection.prepare(&query)?;
            let mut scenarios = statement
                .query_map([], scan_scenario)?
                .collect::<Result<Vec<_>, _>>()?;
            for scenario in &mut scenarios {
                scenario.next_due_unix = next_due(connection, &scenario.id)?;
            }
            Ok(scenarios)
        })
    }

    /// Replaces a scenario definition and rebuilds its schedules in the same
    /// transaction. `expected_revision = 0` disables the concurrency check.
    pub fn update_scenario(
        &self,
        input: &Scenario,
        expected_revision: i64,
        now_unix: i64,
    ) -> Result<Scenario, StoreError> {
        validate_id(&input.id)?;
        let mut prepared = prepare_scenario(self, input)?;
        let next = next_due_of(&prepared, now_unix)?;
        let ring_window = self.config().ring_window;
        self.with_tx(move |tx| {
            let existing = get_scenario_tx(tx, &prepared.id)?;
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
            prepared.next_due_unix = next;
            let next =
                materialize_schedules(tx, &prepared, now_unix, ring_window, DETAIL_RULE_CHANGED)?;
            update_scenario_row(tx, &prepared)?;
            prepared.next_due_unix = next;
            Ok(prepared)
        })
    }

    /// Deletes a scenario and its materialized schedules. A repeated delete is
    /// idempotent and returns `false`.
    pub fn delete_scenario(&self, id: &str, now_unix: i64) -> Result<bool, StoreError> {
        validate_id(id)?;
        let ring_window = self.config().ring_window;
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let scenario = match get_scenario_tx(tx, &id) {
                Ok(scenario) => scenario,
                Err(StoreError::NotFound) => return Ok(false),
                Err(error) => return Err(error),
            };
            remove_scenario_schedules(
                tx,
                &scenario,
                DETAIL_SCENARIO_DELETED,
                now_unix,
                ring_window,
            )?;
            tx.execute("DELETE FROM scenarios WHERE id = ?", params![scenario.id])?;
            Ok(true)
        })
    }

    /// Toggles a scenario and rebuilds (or removes) its schedules.
    pub fn set_scenario_enabled(
        &self,
        id: &str,
        enabled: bool,
        now_unix: i64,
    ) -> Result<Scenario, StoreError> {
        validate_id(id)?;
        let ring_window = self.config().ring_window;
        let id = id.to_owned();
        self.with_tx(move |tx| {
            let mut current = get_scenario_tx(tx, &id)?;
            if current.enabled == enabled {
                return Ok(current);
            }
            current.enabled = enabled;
            current.revision += 1;
            current.updated_unix = now_unix;
            current.next_due_unix = next_due_of(&current, now_unix)?;
            let next =
                materialize_schedules(tx, &current, now_unix, ring_window, DETAIL_RULE_CHANGED)?;
            update_scenario_row(tx, &current)?;
            current.next_due_unix = next;
            Ok(current)
        })
    }

    /// Queues a manual `scenario.fire` event. A non-empty idempotency key
    /// replays the stored event instead of queueing another run.
    pub fn run_scenario(
        &self,
        id: &str,
        idempotency_key: &str,
        now_unix: i64,
    ) -> Result<Event, StoreError> {
        validate_id(id)?;
        validate_idempotency_key(idempotency_key)?;
        let scenario_id = id.to_owned();
        let key = idempotency_key.to_owned();
        let hash = sha1_hex(&format!("scenario_run|{scenario_id}"));
        self.with_tx(move |tx| {
            let scenario = get_scenario_tx(tx, &scenario_id)?;
            if !key.is_empty()
                && let Some((kind, resource_id, existing_hash)) = idempotency_lookup(tx, &key)?
            {
                if kind != "scenario_run" || existing_hash != hash {
                    return Err(StoreError::domain(
                        ErrorKind::Idempotency,
                        "idempotency key reused with a different request",
                    ));
                }
                let sql = format!("SELECT {} FROM events WHERE id=?", super::event::EVENT_COLS);
                return match tx.query_row(&sql, params![resource_id], super::event::scan_event) {
                    Ok(row) => Ok(row.event),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Err(StoreError::domain(
                        ErrorKind::Idempotency,
                        "idempotent scenario run no longer exists",
                    )),
                    Err(error) => Err(error.into()),
                };
            }
            let payload = serde_json::json!({
                "scenario_id": scenario_id,
                "trigger": "manual",
            })
            .to_string();
            let event = insert_event(
                tx,
                EventInsert {
                    dedupe_key: scenario_manual_key(&scenario_id, &random_id()),
                    source_id: scenario_id.clone(),
                    source_kind: SourceKind::Scenario,
                    action: EventAction::Fire,
                    due_unix: now_unix,
                    occurrence_unix: 0,
                    label: scenario.label,
                    payload_json: payload,
                    state: EventState::Pending,
                    created_unix: now_unix,
                    task_type: default_task_type(SourceKind::Scenario, EventAction::Fire),
                    queue: default_queue(SourceKind::Scenario).to_owned(),
                    ..EventInsert::default()
                },
            )?;
            if !key.is_empty() {
                tx.execute(
                    r"INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)",
                    params![key, "scenario_run", event.id, hash, now_unix],
                )
                .map_err(|_| {
                    StoreError::domain(ErrorKind::Idempotency, "idempotency key already exists")
                })?;
            }
            Ok(event)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Repeat, ScenarioCondition, ScenarioStep, ScenarioTimeTrigger, ScenarioTrigger, Schedule,
        ScheduleKind, WallTime,
    };
    use crate::store::{Config, EventAction};

    use crate::domain::AckOutcome;

    const BASE: i64 = 1_767_268_800; // 2026-01-01T12:00:00Z

    fn store() -> Store {
        Store::open_in_memory(Config::default()).expect("store")
    }

    fn daily(hour: i32, minute: i32) -> ScenarioTrigger {
        ScenarioTrigger::Time(ScenarioTimeTrigger {
            timezone: "UTC".to_owned(),
            at: WallTime {
                hour,
                minute,
                second: 0,
            },
            repeat: Repeat::Daily,
            once_date: String::new(),
            weekdays: Vec::new(),
        })
    }

    fn scenario(label: &str, enabled: bool) -> Scenario {
        Scenario {
            label: label.to_owned(),
            description: "turn things on".to_owned(),
            enabled,
            triggers: vec![ScenarioTrigger::Manual, daily(7, 30)],
            conditions: vec![ScenarioCondition::EntityState {
                entity_id: "light.kitchen".to_owned(),
                state: "off".to_owned(),
            }],
            steps: vec![
                ScenarioStep::Catalog {
                    entry_id: "light.kitchen".to_owned(),
                    action: "turn_on".to_owned(),
                    params_json: "{\"brightness\":40}".to_owned(),
                },
                ScenarioStep::Delay { seconds: 5 },
                ScenarioStep::Speak {
                    text: "Готово".to_owned(),
                },
            ],
            continue_on_error: true,
            ..Scenario::default()
        }
    }

    #[test]
    fn crud_materializes_time_triggers_and_rebuilds_them() {
        let store = store();
        let created = store
            .create_scenario(&scenario("evening", true), "", BASE)
            .expect("create");
        assert_eq!(created.revision, 1);
        assert_eq!(created.next_due_unix, 1_767_339_000);

        let schedules = store
            .list_schedules(true, ScheduleKind::Scenario)
            .expect("scenario schedules");
        assert_eq!(schedules.len(), 1);
        assert_eq!(
            schedules[0].payload_json,
            format!(r#"{{"scenario_id":"{}","trigger_index":1}}"#, created.id)
        );

        let loaded = store.get_scenario(&created.id).expect("get");
        assert_eq!(loaded, created);

        let updated = Scenario {
            label: "evening v2".to_owned(),
            triggers: vec![daily(8, 15)],
            ..loaded
        };
        let updated = store
            .update_scenario(&updated, created.revision, BASE)
            .expect("update");
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.next_due_unix, 1_767_341_700);
        let schedules = store
            .list_schedules(true, ScheduleKind::Scenario)
            .expect("rebuilt schedules");
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].label, "evening v2");

        assert!(store.delete_scenario(&created.id, BASE).expect("delete"));
        assert!(
            !store
                .delete_scenario(&created.id, BASE)
                .expect("idempotent delete")
        );
        assert!(store.get_scenario(&created.id).is_err());
        assert!(
            store
                .list_schedules(true, ScheduleKind::Scenario)
                .expect("schedules")
                .is_empty()
        );
    }

    #[test]
    fn revision_conflict_is_reported() {
        let store = store();
        let created = store
            .create_scenario(&scenario("evening", true), "", BASE)
            .expect("create");
        let error = store
            .update_scenario(&created, created.revision + 5, BASE)
            .expect_err("conflict");
        assert!(error.to_string().starts_with("revision conflict:"));
    }

    #[test]
    fn create_is_idempotent() {
        let store = store();
        let first = store
            .create_scenario(&scenario("evening", true), "scenario-key", BASE)
            .expect("create");
        let replay = store
            .create_scenario(&scenario("evening", true), "scenario-key", BASE)
            .expect("replay");
        assert_eq!(first.id, replay.id);
        assert_eq!(first.revision, replay.revision);

        let error = store
            .create_scenario(&scenario("morning", true), "scenario-key", BASE)
            .expect_err("different request");
        assert!(error.to_string().starts_with("idempotency key conflict:"));
    }

    #[test]
    fn enable_toggle_rebuilds_schedules() {
        let store = store();
        let created = store
            .create_scenario(&scenario("evening", true), "", BASE)
            .expect("create");
        let disabled = store
            .set_scenario_enabled(&created.id, false, BASE)
            .expect("disable");
        assert!(!disabled.enabled);
        assert_eq!(disabled.next_due_unix, 0);
        let disabled_schedules = store
            .list_schedules(true, ScheduleKind::Scenario)
            .expect("schedules");
        assert_eq!(disabled_schedules.len(), 1);
        assert!(!disabled_schedules[0].enabled);
        assert_eq!(disabled_schedules[0].next_due_unix, 0);
        let enabled = store
            .set_scenario_enabled(&created.id, true, BASE)
            .expect("enable");
        assert!(enabled.enabled);
        assert_eq!(enabled.next_due_unix, 1_767_339_000);
        let enabled_schedules = store
            .list_schedules(true, ScheduleKind::Scenario)
            .expect("schedules");
        assert_eq!(enabled_schedules.len(), 1);
        assert!(enabled_schedules[0].enabled);
    }

    #[test]
    fn manual_run_queues_a_scenario_event() {
        let store = store();
        let created = store
            .create_scenario(&scenario("evening", false), "", BASE)
            .expect("create");
        let event = store.run_scenario(&created.id, "", BASE).expect("run");
        assert_eq!(event.source_kind, SourceKind::Scenario);
        assert_eq!(event.action, EventAction::Fire);
        assert_eq!(event.task_type, "scenario.fire");
        assert_eq!(event.queue, "scenario");
        assert_eq!(
            event.payload_json,
            format!(r#"{{"scenario_id":"{}","trigger":"manual"}}"#, created.id)
        );
        let due = store
            .list_due_events("scenario-worker", 60, 50, "scenario", BASE)
            .expect("lease");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, event.id);
    }

    #[test]
    fn manual_run_honours_idempotency_key() {
        let store = store();
        let created = store
            .create_scenario(&scenario("evening", false), "", BASE)
            .expect("create");
        let first = store
            .run_scenario(&created.id, "run-key", BASE)
            .expect("run");
        let replay = store
            .run_scenario(&created.id, "run-key", BASE + 10)
            .expect("replay");
        assert_eq!(first.id, replay.id);
        let pending = store
            .list_due_events("scenario-worker", 60, 50, "scenario", BASE + 10)
            .expect("lease");
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn validation_limits_are_enforced() {
        let store = store();
        let mut bad = scenario("evening", true);
        bad.label = "x".repeat(81);
        assert!(store.create_scenario(&bad, "", BASE).is_err());

        let mut bad = scenario("evening", true);
        bad.steps = (0..33)
            .map(|_| ScenarioStep::Delay { seconds: 1 })
            .collect();
        assert!(store.create_scenario(&bad, "", BASE).is_err());

        let mut bad = scenario("evening", true);
        bad.steps = vec![ScenarioStep::Speak {
            text: "x".repeat(501),
        }];
        assert!(store.create_scenario(&bad, "", BASE).is_err());

        let mut bad = scenario("evening", true);
        bad.steps = vec![ScenarioStep::Speak {
            text: "line\nbreak".to_owned(),
        }];
        assert!(store.create_scenario(&bad, "", BASE).is_err());

        let mut bad = scenario("evening", true);
        bad.steps = vec![ScenarioStep::Delay { seconds: 0 }];
        assert!(store.create_scenario(&bad, "", BASE).is_err());

        let mut bad = scenario("evening", true);
        bad.conditions = vec![ScenarioCondition::EntityState {
            entity_id: "Kitchen Light".to_owned(),
            state: "on".to_owned(),
        }];
        assert!(store.create_scenario(&bad, "", BASE).is_err());

        let mut bad = scenario("evening", true);
        bad.triggers = vec![ScenarioTrigger::Time(ScenarioTimeTrigger {
            timezone: "Mars/Olympus".to_owned(),
            at: WallTime {
                hour: 7,
                minute: 0,
                second: 0,
            },
            repeat: Repeat::Daily,
            once_date: String::new(),
            weekdays: Vec::new(),
        })];
        assert!(store.create_scenario(&bad, "", BASE).is_err());
    }

    #[test]
    fn editing_an_active_occurrence_queues_a_stop() {
        let store = store();
        let created = store
            .create_scenario(&scenario("evening", true), "", BASE)
            .expect("create");
        let fire_at = created.next_due_unix;
        store.tick(fire_at).expect("tick");
        let fired = store.get_scenario(&created.id).expect("after tick");
        let schedules = store
            .list_schedules(true, ScheduleKind::Scenario)
            .expect("schedules");
        assert_eq!(schedules[0].active_occurrence_unix, fire_at);

        let events = store
            .list_due_events("scenario-worker", 60, 50, "scenario", fire_at)
            .expect("lease fire");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, EventAction::Fire);
        store
            .ack_event(
                &events[0].id,
                "scenario-worker",
                AckOutcome::Delivered,
                "",
                fire_at,
            )
            .expect("ack");

        let edited = Scenario {
            label: "evening edited".to_owned(),
            ..fired.clone()
        };
        store
            .update_scenario(&edited, fired.revision, fire_at + 10)
            .expect("update");

        let stop = store
            .list_due_events("scenario-worker", 60, 50, "scenario", fire_at + 10)
            .expect("lease stop");
        assert_eq!(stop.len(), 1);
        assert_eq!(stop[0].action, EventAction::Stop);
        assert_eq!(stop[0].task_type, "scenario.stop");
        assert_eq!(stop[0].occurrence_unix, fire_at);
        assert_eq!(stop[0].detail, DETAIL_RULE_CHANGED);
    }

    #[test]
    fn scheduler_service_cannot_mutate_scenario_schedules() {
        let store = store();
        let scenario = store
            .create_scenario(&scenario("evening", true), "", BASE)
            .expect("create");
        let schedules = store
            .list_schedules(true, ScheduleKind::Scenario)
            .expect("scenario schedules");
        assert_eq!(schedules.len(), 1);
        let owned = &schedules[0];
        let expected = format!("precondition failed: managed by scenario {}", scenario.id);

        let error = store
            .update_schedule(owned, 0, BASE)
            .expect_err("update is managed");
        assert_eq!(error.to_string(), expected);
        let error = store
            .set_schedule_enabled(&owned.id, false, BASE)
            .expect_err("enable is managed");
        assert_eq!(error.to_string(), expected);
        let error = store
            .delete_schedule(&owned.id, BASE)
            .expect_err("delete is managed");
        assert_eq!(error.to_string(), expected);
        let error = store
            .snooze_schedule(&owned.id, 5, BASE)
            .expect_err("snooze is managed");
        assert_eq!(error.to_string(), expected);
        let error = store
            .stop_schedule(&owned.id, BASE)
            .expect_err("stop is managed");
        assert_eq!(error.to_string(), expected);

        let direct = Schedule {
            label: "direct".to_owned(),
            kind: ScheduleKind::Scenario,
            timezone: "UTC".to_owned(),
            at: WallTime {
                hour: 7,
                minute: 0,
                second: 0,
            },
            repeat: Repeat::Daily,
            enabled: true,
            ..Schedule::default()
        };
        let error = store
            .create_schedule(&direct, "", BASE)
            .expect_err("direct create rejected");
        assert_eq!(
            error.to_string(),
            "invalid argument: scenario schedules are created through ScenarioService"
        );

        assert!(
            store
                .list_schedules(true, ScheduleKind::Unspecified)
                .expect("unfiltered list")
                .iter()
                .all(|schedule| schedule.kind != ScheduleKind::Scenario)
        );
        assert_eq!(
            store
                .list_schedules(true, ScheduleKind::Scenario)
                .expect("explicit scenario list")
                .len(),
            1
        );
    }

    #[test]
    fn dst_gap_is_skipped_by_the_shared_rule() {
        let store = store();
        let trigger = ScenarioTrigger::Time(ScenarioTimeTrigger {
            timezone: "Europe/Berlin".to_owned(),
            at: WallTime {
                hour: 2,
                minute: 30,
                second: 0,
            },
            repeat: Repeat::Daily,
            once_date: String::new(),
            weekdays: Vec::new(),
        });
        let expected = match &trigger {
            ScenarioTrigger::Time(time) => time.rule().next(BASE).expect("rule").expect("next"),
            ScenarioTrigger::Manual => unreachable!(),
        };
        let created = store
            .create_scenario(
                &Scenario {
                    label: "dst".to_owned(),
                    enabled: true,
                    triggers: vec![trigger],
                    ..Scenario::default()
                },
                "",
                BASE,
            )
            .expect("create");
        assert_eq!(created.next_due_unix, expected);
    }
}

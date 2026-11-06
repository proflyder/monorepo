//! The atomic engine tick: leases, ring windows, snoozes, schedules, timers.

use rusqlite::{Transaction, params};

use crate::domain::{
    EventAction, EventState, SourceKind, TimerState, classify_due, missed_grace_detail,
    timer_trigger_key,
};

use super::event::{EventInsert, insert_event, release_expired_leases};
use super::schedule::{DETAIL_RING_WINDOW_EXPIRED, SCHEDULE_COLS, close_occurrence, scan_schedule};
use super::timer::{TIMER_COLS, scan_timer};
use super::{Store, StoreError, bool_to_int};

const TICK_BATCH_SIZE: i64 = 200;
const MAX_TICK_BATCHES: usize = 10;

/// Summary of one atomic engine tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickStats {
    pub fired: i64,
    pub skipped: i64,
    pub timers: i64,
    pub expired: i64,
    pub leases_released: i64,
}

impl Store {
    /// Performs one atomic engine pass.
    pub fn tick(&self, now_unix: i64) -> Result<TickStats, StoreError> {
        let cfg = self.config().clone();
        self.with_tx(move |tx| {
            let mut stats = TickStats {
                leases_released: release_expired_leases(tx, now_unix, cfg.max_attempts)?,
                ..TickStats::default()
            };
            expire_ring_windows(tx, now_unix, cfg.ring_window, &mut stats)?;
            activate_snoozes(tx, now_unix, cfg.ring_window)?;
            for _ in 0..MAX_TICK_BATCHES {
                let sql = format!(
                    "SELECT {SCHEDULE_COLS} FROM schedules
                        WHERE enabled=1 AND next_due_unix>0 AND next_due_unix<=? AND deleted_unix=0
                        ORDER BY next_due_unix LIMIT ?"
                );
                let mut statement = tx.prepare(&sql)?;
                let schedules = statement
                    .query_map(params![now_unix, TICK_BATCH_SIZE], scan_schedule)?
                    .collect::<Result<Vec<_>, _>>()?;
                drop(statement);
                if schedules.is_empty() {
                    break;
                }
                let count = schedules.len();
                for schedule in &schedules {
                    tick_schedule(
                        tx,
                        schedule,
                        now_unix,
                        cfg.missed_grace,
                        cfg.ring_window,
                        &mut stats,
                    )?;
                }
                if i64::try_from(count).unwrap_or(0) < TICK_BATCH_SIZE {
                    break;
                }
            }
            for _ in 0..MAX_TICK_BATCHES {
                let sql = format!(
                    "SELECT {TIMER_COLS} FROM timers
                        WHERE state=? AND due_unix>0 AND due_unix<=? ORDER BY due_unix LIMIT ?"
                );
                let mut statement = tx.prepare(&sql)?;
                let timers = statement
                    .query_map(
                        params![
                            i64::from(TimerState::Running.code()),
                            now_unix,
                            TICK_BATCH_SIZE
                        ],
                        scan_timer,
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                drop(statement);
                if timers.is_empty() {
                    break;
                }
                let count = timers.len();
                for timer in &timers {
                    insert_event(
                        tx,
                        EventInsert {
                            dedupe_key: timer_trigger_key(&timer.id, timer.due_unix),
                            source_id: timer.id.clone(),
                            source_kind: SourceKind::Timer,
                            action: EventAction::Fire,
                            due_unix: timer.due_unix,
                            occurrence_unix: 0,
                            label: timer.label.clone(),
                            payload_json: "{}".to_owned(),
                            state: EventState::Pending,
                            created_unix: now_unix,
                            ..EventInsert::default()
                        },
                    )?;
                    tx.execute(
                        "UPDATE timers SET state=?, remaining_seconds=0,
                            revision=revision+1, updated_unix=? WHERE id=? AND state=?",
                        params![
                            i64::from(TimerState::Fired.code()),
                            now_unix,
                            timer.id,
                            i64::from(TimerState::Running.code()),
                        ],
                    )?;
                    stats.timers += 1;
                }
                if i64::try_from(count).unwrap_or(0) < TICK_BATCH_SIZE {
                    break;
                }
            }
            Ok(stats)
        })
    }
}

fn expire_ring_windows(
    tx: &Transaction<'_>,
    now_unix: i64,
    ring_window: i64,
    stats: &mut TickStats,
) -> Result<(), StoreError> {
    for _ in 0..MAX_TICK_BATCHES {
        let sql = format!(
            "SELECT {SCHEDULE_COLS} FROM schedules
                WHERE active_until_unix>0 AND active_until_unix<=? AND deleted_unix=0
                ORDER BY active_until_unix LIMIT ?"
        );
        let mut statement = tx.prepare(&sql)?;
        let expired = statement
            .query_map(params![now_unix, TICK_BATCH_SIZE], scan_schedule)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        if expired.is_empty() {
            break;
        }
        let count = expired.len();
        for schedule in &expired {
            let closed = close_occurrence(
                tx,
                schedule,
                schedule.active_occurrence_unix,
                DETAIL_RING_WINDOW_EXPIRED,
                now_unix,
                ring_window,
            )?;
            if closed {
                stats.expired += 1;
            }
        }
        if i64::try_from(count).unwrap_or(0) < TICK_BATCH_SIZE {
            break;
        }
    }
    Ok(())
}

fn activate_snoozes(
    tx: &Transaction<'_>,
    now_unix: i64,
    ring_window: i64,
) -> Result<(), StoreError> {
    let mut statement = tx.prepare(
        "SELECT DISTINCT source_id, occurrence_unix FROM events
            WHERE state=? AND action=? AND snooze_seq>0 AND due_unix<=?",
    )?;
    let items = statement
        .query_map(
            params![
                i64::from(EventState::Pending.code()),
                i64::from(EventAction::Fire.code()),
                now_unix,
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (source_id, occurrence) in items {
        tx.execute(
            "UPDATE schedules SET active_occurrence_unix=?,
                active_until_unix=?, snooze_until_unix=0, revision=revision+1, updated_unix=?
                WHERE id=? AND snooze_until_unix>0",
            params![
                occurrence,
                now_unix.saturating_add(ring_window),
                now_unix,
                source_id,
            ],
        )?;
    }
    Ok(())
}

fn tick_schedule(
    tx: &Transaction<'_>,
    schedule: &crate::domain::Schedule,
    now_unix: i64,
    missed_grace: i64,
    ring_window: i64,
    stats: &mut TickStats,
) -> Result<(), StoreError> {
    let occurrence = schedule.next_due_unix;
    if occurrence <= 0 {
        return Ok(());
    }
    // A long snooze can reach the next occurrence of the same rule. The
    // snoozed occurrence is already ringing, so coalesce the regular
    // occurrence into it instead of emitting a second FIRE.
    if schedule.active_occurrence_unix > 0 && schedule.active_until_unix > now_unix {
        let (next, exhausted) = fast_forward(schedule, now_unix)?;
        return advance_schedule(tx, schedule, next, exhausted, now_unix, ring_window, false);
    }
    match classify_due(occurrence, now_unix, missed_grace) {
        crate::domain::DueClass::Missed { late_seconds, .. } => {
            insert_event(
                tx,
                EventInsert {
                    dedupe_key: crate::domain::schedule_trigger_key(
                        schedule.kind.source_kind(),
                        &schedule.id,
                        occurrence,
                        0,
                    ),
                    source_id: schedule.id.clone(),
                    source_kind: schedule.kind.source_kind(),
                    action: EventAction::Fire,
                    due_unix: occurrence,
                    occurrence_unix: occurrence,
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: EventState::Skipped,
                    detail: missed_grace_detail(late_seconds),
                    created_unix: now_unix,
                    acked_unix: now_unix,
                    ack_outcome: crate::domain::AckOutcome::Skipped,
                    ..EventInsert::default()
                },
            )?;
            stats.skipped += 1;
            let (next, exhausted) = fast_forward(schedule, now_unix)?;
            advance_schedule(tx, schedule, next, exhausted, now_unix, ring_window, false)
        }
        crate::domain::DueClass::Fire { .. } => {
            insert_event(
                tx,
                EventInsert {
                    dedupe_key: crate::domain::schedule_trigger_key(
                        schedule.kind.source_kind(),
                        &schedule.id,
                        occurrence,
                        0,
                    ),
                    source_id: schedule.id.clone(),
                    source_kind: schedule.kind.source_kind(),
                    action: EventAction::Fire,
                    due_unix: occurrence,
                    occurrence_unix: occurrence,
                    label: schedule.label.clone(),
                    payload_json: schedule.payload_json.clone(),
                    state: EventState::Pending,
                    created_unix: now_unix,
                    ..EventInsert::default()
                },
            )?;
            stats.fired += 1;
            let (next, exhausted) = match schedule.rule().next(occurrence) {
                Ok(Some(next)) => (next, false),
                Ok(None) => (0, true),
                Err(error) => {
                    return Err(crate::domain::DomainError::invalid_rule(&error).into());
                }
            };
            advance_schedule(tx, schedule, next, exhausted, now_unix, ring_window, true)
        }
        crate::domain::DueClass::NotDue => Ok(()),
    }
}

fn fast_forward(
    schedule: &crate::domain::Schedule,
    now_unix: i64,
) -> Result<(i64, bool), StoreError> {
    match schedule.rule().next(now_unix) {
        Ok(Some(next)) => Ok((next, false)),
        Ok(None) => Ok((0, true)),
        Err(error) => Err(crate::domain::DomainError::invalid_rule(&error).into()),
    }
}

fn advance_schedule(
    tx: &Transaction<'_>,
    schedule: &crate::domain::Schedule,
    next: i64,
    exhausted: bool,
    now_unix: i64,
    ring_window: i64,
    fired: bool,
) -> Result<(), StoreError> {
    let (enabled, next) = if exhausted { (false, 0) } else { (true, next) };
    if fired {
        tx.execute(
            "UPDATE schedules SET next_due_unix=?, enabled=?,
                last_fired_unix=?, active_occurrence_unix=?, active_until_unix=?,
                snooze_until_unix=0, revision=revision+1, updated_unix=? WHERE id=?",
            params![
                next,
                bool_to_int(enabled),
                schedule.next_due_unix,
                schedule.next_due_unix,
                now_unix.saturating_add(ring_window),
                now_unix,
                schedule.id,
            ],
        )?;
        return Ok(());
    }
    tx.execute(
        "UPDATE schedules SET next_due_unix=?, enabled=?,
            revision=revision+1, updated_unix=? WHERE id=?",
        params![next, bool_to_int(enabled), now_unix, schedule.id],
    )?;
    Ok(())
}

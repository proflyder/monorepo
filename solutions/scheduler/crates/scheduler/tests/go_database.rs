//! Opens a SQL dump produced by the Go store and reads it through the Rust
//! store. This is the rollback-compatibility test: the PVC format belongs to
//! the Go service until cutover.

use std::path::PathBuf;

use rusqlite::Connection;
use scheduler::domain::{
    Repeat, ScheduleKind, TimerState, WallTime, canonical_schedule, canonical_task,
    canonical_timer, event_id, hash::sha1_hex,
};
use scheduler::store::{Config, EventState, Store};

fn dump_path() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR").map_or_else(
        || PathBuf::from("solutions/scheduler/testdata/go_v1.sql"),
        |manifest| PathBuf::from(manifest).join("../../testdata/go_v1.sql"),
    )
}

struct IdempotencyRow {
    key: String,
    kind: String,
    hash: String,
}

#[test]
fn go_database_opens_and_reads_identically() {
    let sql = std::fs::read_to_string(dump_path()).expect("read go_v1.sql");
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("go-v1.db");
    let connection = Connection::open(&path).expect("open sqlite");
    connection.execute_batch(&sql).expect("apply dump");

    let version: i64 = connection
        .query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    assert_eq!(version, 1, "fixture is a v1 database");

    let idempotency = {
        let mut statement = connection
            .prepare("SELECT key, resource_kind, request_hash FROM idempotency ORDER BY key")
            .expect("prepare idempotency");
        statement
            .query_map([], |row| {
                Ok(IdempotencyRow {
                    key: row.get(0)?,
                    kind: row.get(1)?,
                    hash: row.get(2)?,
                })
            })
            .expect("query idempotency")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect idempotency")
    };
    assert_eq!(idempotency.len(), 3);

    let event_pairs = {
        let mut statement = connection
            .prepare("SELECT id, dedupe_key FROM events ORDER BY id")
            .expect("prepare events");
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query events")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect events")
    };
    assert_eq!(event_pairs.len(), 4);
    for (id, dedupe_key) in &event_pairs {
        assert_eq!(id, &event_id(dedupe_key), "Go event id is UUIDv5 of dedupe");
    }
    drop(connection);

    let store = Store::open(
        &path,
        Config {
            max_attempts: 2,
            ..Config::default()
        },
    )
    .expect("Rust store opens the Go database");

    let schedules = store
        .list_schedules(true, ScheduleKind::Unspecified)
        .expect("list schedules");
    assert_eq!(schedules.len(), 2);
    assert!(
        schedules
            .iter()
            .all(|schedule| schedule.kind != ScheduleKind::Scenario),
        "UNSPECIFIED hides scenario trigger rows"
    );
    let wake = schedules
        .iter()
        .find(|schedule| schedule.label == "wake")
        .expect("wake");
    assert_eq!(wake.kind, ScheduleKind::Alarm);
    assert_eq!(wake.timezone, "UTC");
    assert_eq!(
        wake.at,
        WallTime {
            hour: 7,
            minute: 30,
            second: 0
        }
    );
    assert_eq!(wake.repeat, Repeat::Daily);
    assert!(wake.weekdays.is_empty());
    assert!(wake.enabled);
    assert_eq!(wake.next_due_unix, 1_767_425_400);
    assert_eq!(wake.revision, 2);
    assert_eq!(wake.payload_json, "{}");
    assert_eq!(wake.created_unix, 1_767_268_800);
    assert_eq!(wake.updated_unix, 1_767_339_000);
    assert_eq!(wake.last_fired_unix, 1_767_339_000);
    assert_eq!(wake.active_occurrence_unix, 1_767_339_000);
    assert_eq!(wake.active_until_unix, 1_767_339_900);
    assert_eq!(wake.deleted_unix, 0);

    let reminder = schedules
        .iter()
        .find(|schedule| schedule.label == "call mom")
        .expect("reminder");
    assert_eq!(reminder.kind, ScheduleKind::Reminder);
    assert_eq!(reminder.repeat, Repeat::Once);
    assert_eq!(reminder.once_date, "2026-02-01");
    assert_eq!(reminder.next_due_unix, 1_769_936_400);
    assert_eq!(reminder.revision, 1);

    let scenario_rows = store
        .list_schedules(true, ScheduleKind::Scenario)
        .expect("list scenario schedules");
    assert_eq!(scenario_rows.len(), 1);
    let evening = scenario_rows
        .iter()
        .find(|schedule| schedule.label == "evening")
        .expect("scenario");
    assert_eq!(evening.kind, ScheduleKind::Scenario);
    assert_eq!(evening.timezone, "Europe/Berlin");
    assert!(!evening.enabled);
    assert_eq!(evening.next_due_unix, 0);

    let enabled = store
        .list_schedules(false, ScheduleKind::Unspecified)
        .expect("live schedules");
    assert_eq!(enabled.len(), 2);

    let timers = store.list_timers(true).expect("list timers");
    assert_eq!(timers.len(), 2);
    let tea = timers
        .iter()
        .find(|timer| timer.label == "tea")
        .expect("tea");
    assert_eq!(tea.state, TimerState::Paused);
    assert_eq!(tea.duration_seconds, 300);
    assert_eq!(tea.remaining_seconds, 200);
    assert_eq!(tea.due_unix, 0);
    assert_eq!(tea.revision, 2);
    let coffee = timers
        .iter()
        .find(|timer| timer.label == "coffee")
        .expect("coffee");
    assert_eq!(coffee.state, TimerState::Fired);
    assert_eq!(coffee.due_unix, 1_767_269_400);
    assert_eq!(coffee.revision, 2);
    assert_eq!(store.list_timers(false).expect("active timers").len(), 1);

    let status = store.status(1_767_339_000).expect("status");
    assert_eq!(status.pending_events, 3);
    assert_eq!(status.leased_events, 0);
    assert_eq!(status.enabled_schedules, 2);
    assert_eq!(status.running_timers, 1);
    assert_eq!(status.consumers.len(), 1);
    assert_eq!(status.consumers[0].consumer_id, "home-1");
    assert_eq!(status.consumers[0].last_seen_unix, 1_767_268_800);
    assert_eq!(status.consumers[0].last_ack_unix, 1_767_268_800);
    assert!(!status.consumers[0].alive);
    let queue = |name: &str| {
        status
            .queues
            .iter()
            .find(|queue| queue.name == name)
            .expect("built-in queue")
    };
    assert_eq!(queue("home").pending, 2);
    assert_eq!(queue("tasks").pending, 0);
    assert_eq!(queue("voice-work").pending, 1);
    assert_eq!(queue("scenario").pending, 0);

    let events = store
        .list_due_events("reader", 60, 50, "", 1_767_339_000)
        .expect("lease due events");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].task_type, "demo.run");
    assert_eq!(events[0].queue, "voice-work");
    assert_eq!(events[0].state, EventState::Leased);
    assert_eq!(events[1].task_type, "timer.fire");
    assert_eq!(events[1].label, "coffee");
    assert_eq!(events[1].source_kind, scheduler::domain::SourceKind::Timer);
    assert_eq!(events[2].task_type, "alarm.fire");
    assert_eq!(events[2].occurrence_unix, 1_767_339_000);

    let schedule_hash = sha1_hex(&canonical_schedule(
        "wake",
        ScheduleKind::Alarm,
        "UTC",
        "",
        WallTime {
            hour: 7,
            minute: 30,
            second: 0,
        },
        Repeat::Daily,
        &[],
        true,
        "{}",
    ));
    let timer_hash = sha1_hex(&canonical_timer("tea", 300));
    let task_hash = sha1_hex(&canonical_task("notify.push", "tasks", "{}", 0));
    let idempotency_row = |key: &str| {
        idempotency
            .iter()
            .find(|row| row.key == key)
            .expect("idempotency row")
    };
    assert_eq!(idempotency_row("sched-key").kind, "schedule");
    assert_eq!(idempotency_row("sched-key").hash, schedule_hash);
    assert_eq!(idempotency_row("timer-key").kind, "timer");
    assert_eq!(idempotency_row("timer-key").hash, timer_hash);
    assert_eq!(idempotency_row("task-key").kind, "task");
    assert_eq!(idempotency_row("task-key").hash, task_hash);

    store.close().expect("close store");

    let migrated = Connection::open(&path).expect("reopen migrated database");
    let version: i64 = migrated
        .query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    assert_eq!(version, 3, "v1 fixture migrates to v3");
    let scenarios: i64 = migrated
        .query_row("SELECT COUNT(*) FROM scenarios", [], |row| row.get(0))
        .expect("scenarios table exists");
    assert_eq!(scenarios, 0);
    let push_devices: i64 = migrated
        .query_row("SELECT COUNT(*) FROM push_devices", [], |row| row.get(0))
        .expect("push_devices table exists");
    assert_eq!(push_devices, 0);
}

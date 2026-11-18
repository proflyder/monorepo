//! One-time conversion of legacy local-time schedule rows to UTC.
//!
//! Schema v5 adds `once_unix`; this pass rewrites every existing row so that
//! `at_*` and `weekdays` are UTC and `once_unix` is the UTC instant of a
//! one-shot occurrence, then rebuilds the table without the `timezone` and
//! `once_date` columns. IANA conversion lives in Rust because SQLite has no
//! time-zone database.
//!
//! The owner's live alarms are `Asia/Almaty` (UTC+5, no DST), so the expected
//! result is an exact time-of-day shift by five hours with the weekday mask
//! moved back one day where the local time crosses midnight UTC. Zones with
//! DST use the offset of a reference week; a fixed UTC rule cannot represent
//! a wall clock that moves twice a year.

use home_sqlite::{Db, SqliteError};
use rusqlite::{Connection, params};

use crate::domain::hash::{canonical_schedule, sha1_hex};
use crate::domain::rule::{Repeat, WallTime, local_to_utc, weekdays_text};
use crate::domain::schedule::ScheduleKind;

/// Rewrites legacy rows and drops the time-zone columns. Idempotent: a
/// database that already went through this pass has no `timezone` column.
pub fn legacy_schedules_to_utc(db: &Db) -> Result<(), SqliteError> {
    db.call_blocking(migrate)
}

fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    if !has_column(connection, "schedules", "timezone")? {
        return Ok(());
    }
    let legacy = read_legacy(connection)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE schedules_utc (
            id TEXT PRIMARY KEY,
            kind INTEGER NOT NULL,
            label TEXT NOT NULL,
            at_hour INTEGER NOT NULL,
            at_minute INTEGER NOT NULL,
            at_second INTEGER NOT NULL,
            once_unix INTEGER NOT NULL DEFAULT 0,
            repeat_kind INTEGER NOT NULL,
            weekdays TEXT NOT NULL,
            next_due_unix INTEGER NOT NULL,
            enabled INTEGER NOT NULL,
            payload_json TEXT NOT NULL,
            revision INTEGER NOT NULL,
            created_unix INTEGER NOT NULL,
            updated_unix INTEGER NOT NULL,
            deleted_unix INTEGER NOT NULL DEFAULT 0,
            last_fired_unix INTEGER NOT NULL DEFAULT 0,
            snooze_until_unix INTEGER NOT NULL DEFAULT 0,
            snooze_count INTEGER NOT NULL DEFAULT 0,
            active_occurrence_unix INTEGER NOT NULL DEFAULT 0,
            active_until_unix INTEGER NOT NULL DEFAULT 0
        )",
    )?;
    for row in &legacy {
        let rule = local_to_utc(
            &row.timezone,
            row.at,
            row.repeat,
            &row.weekdays,
            &row.once_date,
        );
        transaction.execute(
            "INSERT INTO schedules_utc (
                id, kind, label, at_hour, at_minute, at_second, once_unix, repeat_kind,
                weekdays, next_due_unix, enabled, payload_json, revision, created_unix,
                updated_unix, deleted_unix, last_fired_unix, snooze_until_unix, snooze_count,
                active_occurrence_unix, active_until_unix
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                row.id,
                row.kind,
                row.label,
                i64::from(rule.at.hour),
                i64::from(rule.at.minute),
                i64::from(rule.at.second),
                rule.once_unix,
                i64::from(rule.repeat.code()),
                weekdays_text(&rule.weekdays),
                row.next_due_unix,
                row.enabled,
                row.payload_json,
                row.revision,
                row.created_unix,
                row.updated_unix,
                row.deleted_unix,
                row.last_fired_unix,
                row.snooze_until_unix,
                row.snooze_count,
                row.active_occurrence_unix,
                row.active_until_unix,
            ],
        )?;
        let kind =
            ScheduleKind::from_code(i32::try_from(row.kind).unwrap_or(0)).unwrap_or_default();
        let hash = sha1_hex(&canonical_schedule(
            &row.label,
            kind,
            rule.once_unix,
            rule.at,
            rule.repeat,
            &rule.weekdays,
            row.enabled != 0,
            &row.payload_json,
        ));
        if has_table(&transaction, "idempotency")? {
            transaction.execute(
                "UPDATE idempotency SET request_hash = ? WHERE resource_kind = 'schedule' AND resource_id = ?",
                params![hash, row.id],
            )?;
        }
    }
    transaction.execute_batch(
        "DROP TABLE schedules;
         ALTER TABLE schedules_utc RENAME TO schedules;
         CREATE INDEX IF NOT EXISTS schedules_due ON schedules(enabled, next_due_unix);",
    )?;
    transaction.commit()
}

fn has_table(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        params![table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn has_column(connection: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

struct LegacyRow {
    id: String,
    kind: i64,
    label: String,
    timezone: String,
    at: WallTime,
    once_date: String,
    repeat: Repeat,
    weekdays: Vec<i32>,
    next_due_unix: i64,
    enabled: i64,
    payload_json: String,
    revision: i64,
    created_unix: i64,
    updated_unix: i64,
    deleted_unix: i64,
    last_fired_unix: i64,
    snooze_until_unix: i64,
    snooze_count: i64,
    active_occurrence_unix: i64,
    active_until_unix: i64,
}

fn read_legacy(connection: &Connection) -> rusqlite::Result<Vec<LegacyRow>> {
    let mut statement = connection.prepare(
        "SELECT id, kind, label, timezone, at_hour, at_minute, at_second, once_date,
            repeat_kind, weekdays, next_due_unix, enabled, payload_json, revision,
            created_unix, updated_unix, deleted_unix, last_fired_unix, snooze_until_unix,
            snooze_count, active_occurrence_unix, active_until_unix
         FROM schedules",
    )?;
    let rows = statement.query_map([], |row| {
        let weekdays: String = row.get(9)?;
        Ok(LegacyRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            label: row.get(2)?,
            timezone: row.get(3)?,
            at: WallTime {
                hour: i32::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                minute: i32::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                second: i32::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
            },
            once_date: row.get(7)?,
            repeat: Repeat::from_code(i32::try_from(row.get::<_, i64>(8)?).unwrap_or(0))
                .unwrap_or_default(),
            weekdays: weekdays
                .split(',')
                .filter_map(|part| part.trim().parse::<i32>().ok())
                .collect(),
            next_due_unix: row.get(10)?,
            enabled: row.get(11)?,
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
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use home_sqlite::{Db, Migration};

    const V1: &[&str] = &[
        "CREATE TABLE schedules (
            id TEXT PRIMARY KEY,
            kind INTEGER NOT NULL,
            label TEXT NOT NULL,
            timezone TEXT NOT NULL,
            at_hour INTEGER NOT NULL,
            at_minute INTEGER NOT NULL,
            at_second INTEGER NOT NULL,
            once_date TEXT NOT NULL,
            repeat_kind INTEGER NOT NULL,
            weekdays TEXT NOT NULL,
            next_due_unix INTEGER NOT NULL,
            enabled INTEGER NOT NULL,
            payload_json TEXT NOT NULL,
            revision INTEGER NOT NULL,
            created_unix INTEGER NOT NULL,
            updated_unix INTEGER NOT NULL,
            deleted_unix INTEGER NOT NULL DEFAULT 0,
            last_fired_unix INTEGER NOT NULL DEFAULT 0,
            snooze_until_unix INTEGER NOT NULL DEFAULT 0,
            snooze_count INTEGER NOT NULL DEFAULT 0,
            active_occurrence_unix INTEGER NOT NULL DEFAULT 0,
            active_until_unix INTEGER NOT NULL DEFAULT 0
        )",
        "CREATE INDEX schedules_due ON schedules(enabled, next_due_unix)",
    ];
    const V5: &[&str] = &["ALTER TABLE schedules ADD COLUMN once_unix INTEGER NOT NULL DEFAULT 0"];
    const MIGRATIONS: &[Migration] = &[
        Migration {
            version: 1,
            description: "legacy",
            statements: V1,
        },
        Migration {
            version: 2,
            description: "utc",
            statements: V5,
        },
    ];

    #[test]
    fn almaty_weekdays_move_to_sunday_thursday_utc() {
        let db = Db::open_in_memory(MIGRATIONS).expect("open");
        db.call_blocking(|connection| {
            connection.execute(
                "INSERT INTO schedules (
                    id, kind, label, timezone, at_hour, at_minute, at_second, once_date,
                    repeat_kind, weekdays, next_due_unix, enabled, payload_json, revision,
                    created_unix, updated_unix
                ) VALUES ('a', 1, 'wake', 'Asia/Almaty', 3, 0, 0, '', 3, '', 0, 1, '{}', 1, 0, 0)",
                [],
            )?;
            connection.execute(
                "INSERT INTO schedules (
                    id, kind, label, timezone, at_hour, at_minute, at_second, once_date,
                    repeat_kind, weekdays, next_due_unix, enabled, payload_json, revision,
                    created_unix, updated_unix
                ) VALUES ('b', 1, 'once', 'Asia/Almaty', 9, 0, 0, '2026-02-01', 1, '', 1769936400, 1, '{}', 1, 0, 0)",
                [],
            )?;
            Ok(())
        })
        .expect("seed");
        legacy_schedules_to_utc(&db).expect("migrate");
        let row = db
            .call_blocking(|connection| {
                connection.query_row(
                    "SELECT at_hour, weekdays, once_unix FROM schedules WHERE id = 'a'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
            })
            .expect("read");
        assert_eq!(row, (22, "1,2,3,4,7".to_owned(), 0));
        let once = db
            .call_blocking(|connection| {
                connection.query_row(
                    "SELECT at_hour, once_unix FROM schedules WHERE id = 'b'",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
            })
            .expect("read");
        assert_eq!(once, (0, 1_769_918_400));
        let has_timezone = db
            .call_blocking(|connection| has_column(connection, "schedules", "timezone"))
            .expect("pragma");
        assert!(!has_timezone, "legacy columns are dropped");
    }
}

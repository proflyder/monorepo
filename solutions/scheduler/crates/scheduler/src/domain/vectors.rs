//! Golden vectors shared with the former Go implementation.
//!
//! The JSON files are read from `solutions/scheduler/testdata`, never copied:
//! `testdata/planner/*.json` proves recurrence/DST parity and `ids.json`
//! proves every dedupe-key format and canonical SHA-1.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;

fn testdata_dir() -> PathBuf {
    let relative = Path::new("solutions/scheduler/testdata");
    if std::env::var_os("TEST_SRCDIR").is_none() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        return manifest.join("../../testdata");
    }
    for root in ["TEST_SRCDIR", "RUNFILES_DIR", "GULP_SRCDIR"] {
        let Ok(root) = std::env::var(root) else {
            continue;
        };
        let workspace = std::env::var("TEST_WORKSPACE").unwrap_or_default();
        let candidate = Path::new(&root).join(&workspace).join(relative);
        if candidate.is_dir() {
            return candidate;
        }
        let candidate = Path::new(&root).join(relative);
        if candidate.is_dir() {
            return candidate;
        }
    }
    relative.to_owned()
}

fn read_json(path: &Path) -> Value {
    let bytes =
        std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn field_i64(value: &Value, name: &str) -> i64 {
    value
        .get(name)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("missing integer field {name} in {value}"))
}

#[test]
fn planner_vectors_match_go() {
    let dir = testdata_dir().join("planner");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no planner vectors in {}", dir.display());

    let mut cases = 0;
    for file in &files {
        let vectors = read_json(file);
        let vectors = vectors
            .as_array()
            .unwrap_or_else(|| panic!("{} is not an array", file.display()));
        for vector in vectors {
            let name = vector
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("<unnamed>");
            let raw_rule = vector.get("rule").expect("rule");
            let repeat_name = raw_rule
                .get("repeat")
                .and_then(Value::as_str)
                .expect("repeat");
            let rule = Rule {
                timezone: raw_rule
                    .get("timezone")
                    .and_then(Value::as_str)
                    .expect("timezone")
                    .to_owned(),
                at: WallTime {
                    hour: i32::try_from(field_i64(raw_rule, "hour")).expect("hour"),
                    minute: i32::try_from(field_i64(raw_rule, "minute")).expect("minute"),
                    second: i32::try_from(field_i64(raw_rule, "second")).expect("second"),
                },
                repeat: Repeat::from_name(repeat_name)
                    .unwrap_or_else(|| panic!("unknown repeat {repeat_name}")),
                weekdays: raw_rule
                    .get("weekdays")
                    .and_then(Value::as_array)
                    .map(|days| {
                        days.iter()
                            .map(|day| {
                                i32::try_from(day.as_i64().expect("weekday")).expect("range")
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                once_date: raw_rule
                    .get("onceDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            };
            rule.validate()
                .unwrap_or_else(|error| panic!("{} / {name}: {error}", file.display()));
            let after = field_i64(vector, "afterUnix");
            let expected = vector.get("expectedUnix").and_then(Value::as_i64);
            let actual = rule
                .next(after)
                .unwrap_or_else(|error| panic!("{} / {name}: {error}", file.display()));
            assert_eq!(
                actual,
                expected,
                "{} / {name}: rule {rule:?} after {after}",
                file.display()
            );
            cases += 1;
        }
    }
    assert!(cases >= 16, "expected the full planner table, got {cases}");
}

#[test]
fn dedupe_keys_and_event_ids_match_go() {
    let ids = read_json(&testdata_dir().join("ids.json"));
    let events = ids
        .get("events")
        .and_then(Value::as_array)
        .expect("events array");

    let mut checked = 0;
    for event in events {
        let name = event.get("name").and_then(Value::as_str).expect("name");
        let dedupe_key = event
            .get("dedupeKey")
            .and_then(Value::as_str)
            .expect("dedupeKey");
        let uuid = event.get("uuid").and_then(Value::as_str).expect("uuid");

        let built = match name {
            "alarm occurrence" => {
                schedule_trigger_key(SourceKind::Alarm, "sched-alarm", 1_767_339_000, 0)
            }
            "reminder occurrence" => {
                schedule_trigger_key(SourceKind::Reminder, "sched-reminder", 1_767_339_000, 0)
            }
            "timer literal kind" => timer_trigger_key("timer-1", 1_767_269_100),
            "scenario occurrence" => {
                schedule_trigger_key(SourceKind::Scenario, "sched-scenario", 1_767_339_000, 0)
            }
            "control stop schedule" => schedule_control_stop_key("sched-alarm", 1_767_339_000, 0),
            "control stop schedule snooze" => {
                schedule_control_stop_key("sched-alarm", 1_767_339_000, 1)
            }
            "snoozed fire" => {
                schedule_trigger_key(SourceKind::Alarm, "sched-alarm", 1_767_339_000, 1)
            }
            "expiry stop" => expiry_stop_key(SourceKind::Alarm, "sched-alarm", 1_767_339_000),
            "timer cancel stop" => timer_control_stop_key("timer-1", 1_767_269_100),
            "task event" => task_dedupe_key("019b796d-d600-77c5-89f1-c06bc947cfc7"),
            other => panic!("unexpected event vector {other}"),
        };
        assert_eq!(built, dedupe_key, "dedupe key format for {name}");
        assert_eq!(event_id(dedupe_key), uuid, "uuid for {name}");
        checked += 1;
    }
    assert_eq!(checked, 10, "all dedupe formats covered");
}

#[test]
fn canonical_hashes_match_go() {
    let ids = read_json(&testdata_dir().join("ids.json"));
    let hashes = ids
        .get("hashes")
        .and_then(Value::as_array)
        .expect("hashes array");

    let mut checked = 0;
    for hash in hashes {
        let name = hash.get("name").and_then(Value::as_str).expect("name");
        let canonical = hash
            .get("canonical")
            .and_then(Value::as_str)
            .expect("canonical");
        let expected = hash.get("sha1").and_then(Value::as_str).expect("sha1");

        let built = match name {
            "canonical daily alarm" => canonical_schedule(
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
            ),
            "canonical custom weekdays sorted" => canonical_schedule(
                "gym",
                ScheduleKind::Alarm,
                "Asia/Almaty",
                "",
                WallTime {
                    hour: 6,
                    minute: 15,
                    second: 5,
                },
                Repeat::Custom,
                &[5, 1, 3],
                false,
                "{\"source\":\"test\"}",
            ),
            "canonical once reminder" => canonical_schedule(
                "call mom",
                ScheduleKind::Reminder,
                "Europe/Berlin",
                "2026-02-01",
                WallTime {
                    hour: 9,
                    minute: 0,
                    second: 0,
                },
                Repeat::Once,
                &[],
                true,
                "{}",
            ),
            "canonical timer" => canonical_timer("tea", 300),
            "canonical task" => {
                canonical_task("notify.push", "tasks", "{\"text\":\"hi\"}", 1_767_268_800)
            }
            other => panic!("unexpected hash vector {other}"),
        };
        assert_eq!(built, canonical, "canonical string for {name}");
        assert_eq!(sha1_hex(&built), expected, "sha1 for {name}");
        checked += 1;
    }
    assert_eq!(checked, 5, "all canonical formats covered");
}

#[test]
fn rule_validation_messages_match_go() {
    let valid = Rule {
        timezone: "UTC".to_owned(),
        at: WallTime {
            hour: 7,
            minute: 0,
            second: 0,
        },
        repeat: Repeat::Daily,
        weekdays: Vec::new(),
        once_date: String::new(),
    };
    assert!(valid.validate().is_ok());

    let cases = [
        (
            Rule {
                timezone: String::new(),
                ..valid.clone()
            },
            "invalid schedule rule: timezone is required",
        ),
        (
            Rule {
                timezone: "Mars/Olympus".to_owned(),
                ..valid.clone()
            },
            "invalid schedule rule: unknown timezone \"Mars/Olympus\"",
        ),
        (
            Rule {
                at: WallTime {
                    hour: 24,
                    minute: 0,
                    second: 0,
                },
                ..valid.clone()
            },
            "invalid schedule rule: hour 24 outside 0..23",
        ),
        (
            Rule {
                repeat: Repeat::Once,
                ..valid.clone()
            },
            "invalid schedule rule: once_date is required for REPEAT_KIND_ONCE",
        ),
        (
            Rule {
                repeat: Repeat::Custom,
                ..valid.clone()
            },
            "invalid schedule rule: REPEAT_KIND_CUSTOM needs weekdays",
        ),
        (
            Rule {
                repeat: Repeat::Custom,
                weekdays: vec![1, 8],
                ..valid.clone()
            },
            "invalid schedule rule: weekday 8 outside 1..7",
        ),
        (
            Rule {
                repeat: Repeat::Custom,
                weekdays: vec![1, 1],
                ..valid.clone()
            },
            "invalid schedule rule: duplicate weekday 1",
        ),
        (
            Rule {
                once_date: "2026-01-05".to_owned(),
                ..valid.clone()
            },
            "invalid schedule rule: once_date is only valid for REPEAT_KIND_ONCE",
        ),
        (
            Rule {
                repeat: Repeat::Unspecified,
                ..valid
            },
            "invalid schedule rule: unsupported repeat REPEAT_KIND_UNSPECIFIED",
        ),
    ];
    for (rule, expected) in cases {
        assert_eq!(rule.validate().expect_err("invalid").to_string(), expected);
    }
}

#[test]
fn once_date_rejects_non_iso_dates() {
    for date in [
        "2026-1-05",
        "2026-01-5",
        "2026-1-005",
        "2026-02-30",
        "05.01.2026",
        "",
    ] {
        let rule = Rule {
            timezone: "UTC".to_owned(),
            at: WallTime {
                hour: 9,
                minute: 0,
                second: 0,
            },
            repeat: Repeat::Once,
            weekdays: Vec::new(),
            once_date: date.to_owned(),
        };
        assert!(rule.validate().is_err(), "{date} must be rejected");
    }
}

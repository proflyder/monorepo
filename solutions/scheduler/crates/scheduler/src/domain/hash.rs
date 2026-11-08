//! Canonical idempotency strings and their SHA-1 digests.
//!
//! The strings are the Go `canonicalSchedule`/`canonicalTimer`/`canonicalTask`
//! format byte for byte: field order, empty fields and `true`/`false` matter
//! because the digest is persisted in the `idempotency` table.

use sha1_smol::Sha1;

use crate::domain::rule::{Repeat, WallTime, weekdays_text};
use crate::domain::schedule::ScheduleKind;

/// Canonical schedule request.
///
/// The nine fields are the Go format arguments; grouping them would hide the
/// exact layout that the persisted SHA-1 depends on.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn canonical_schedule(
    label: &str,
    kind: ScheduleKind,
    timezone: &str,
    once_date: &str,
    at: WallTime,
    repeat: Repeat,
    weekdays: &[i32],
    enabled: bool,
    payload_json: &str,
) -> String {
    format!(
        "schedule|{label}|{}|{timezone}|{once_date}|{}|{}|{}|{}|{}|{enabled}|{payload_json}",
        kind.code(),
        at.hour,
        at.minute,
        at.second,
        repeat.code(),
        weekdays_text(weekdays),
    )
}

/// Canonical timer request.
#[must_use]
pub fn canonical_timer(label: &str, duration_seconds: i64) -> String {
    format!("timer|{label}|{duration_seconds}")
}

/// Canonical generic task request. `run_at_unix` is the request value, not the
/// clamped due time.
#[must_use]
pub fn canonical_task(
    task_type: &str,
    queue: &str,
    payload_json: &str,
    run_at_unix: i64,
) -> String {
    format!("task|{task_type}|{queue}|{payload_json}|{run_at_unix}")
}

/// Canonical scenario definition used for idempotent CreateScenario.
#[must_use]
pub fn canonical_scenario(body_json: &str, enabled: bool) -> String {
    format!("scenario|{enabled}|{body_json}")
}

/// Lower-case hex SHA-1 of a canonical request string.
#[must_use]
pub fn sha1_hex(canonical: &str) -> String {
    Sha1::from(canonical).digest().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_schedule_matches_go_layout() {
        let canonical = canonical_schedule(
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
        );
        assert_eq!(canonical, "schedule|wake|1|UTC||7|30|0|2||true|{}");
        assert_eq!(
            sha1_hex(&canonical),
            "b9add2e9dd506dd05051ebbc84f6754c847203f7"
        );
    }

    #[test]
    fn canonical_schedule_sorts_weekdays() {
        let canonical = canonical_schedule(
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
        );
        assert_eq!(
            canonical,
            "schedule|gym|1|Asia/Almaty||6|15|5|5|1,3,5|false|{\"source\":\"test\"}"
        );
    }
}

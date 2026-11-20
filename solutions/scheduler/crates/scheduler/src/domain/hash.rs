use sha1_smol::Sha1;

use crate::domain::rule::{Repeat, WallTime, weekdays_text};
use crate::domain::schedule::ScheduleKind;

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn canonical_schedule(
    label: &str,
    kind: ScheduleKind,
    once_unix: i64,
    at: WallTime,
    repeat: Repeat,
    weekdays: &[i32],
    enabled: bool,
    payload_json: &str,
) -> String {
    format!(
        "schedule|{label}|{}|{once_unix}|{}|{}|{}|{}|{}|{enabled}|{payload_json}",
        kind.code(),
        at.hour,
        at.minute,
        at.second,
        repeat.code(),
        weekdays_text(weekdays),
    )
}

#[must_use]
pub fn canonical_timer(label: &str, duration_seconds: i64) -> String {
    format!("timer|{label}|{duration_seconds}")
}

#[must_use]
pub fn canonical_task(
    task_type: &str,
    queue: &str,
    payload_json: &str,
    run_at_unix: i64,
) -> String {
    format!("task|{task_type}|{queue}|{payload_json}|{run_at_unix}")
}

#[must_use]
pub fn canonical_scenario(body_json: &str, enabled: bool) -> String {
    format!("scenario|{enabled}|{body_json}")
}

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
            0,
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
        assert_eq!(canonical, "schedule|wake|1|0|7|30|0|2||true|{}");
        assert_eq!(
            sha1_hex(&canonical),
            "6977661e122ba3c4868097646f07cfa1ebab9dab"
        );
    }

    #[test]
    fn canonical_schedule_sorts_weekdays() {
        let canonical = canonical_schedule(
            "gym",
            ScheduleKind::Alarm,
            0,
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
            "schedule|gym|1|0|6|15|5|5|1,3,5|false|{\"source\":\"test\"}"
        );
    }
}

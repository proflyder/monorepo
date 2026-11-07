//! Read-only safety checks for `deploy.sh --execute`.
//!
//! Ported from `platform/deploy/internal/preflight`: retired legacy HA writers
//! must be absent or `off`, no alarm may be ringing, the scheduler endpoint must
//! match, and the scheduler must answer ready. The HA token is read from the
//! token file and never printed; any failure exits 1 with the Python messages.

use std::io::Write;

use crate::http::{Client, PREFLIGHT_TIMEOUT};

/// Retired HA schedule writers. A missing entity is safe; an existing one must
/// be exactly `off`.
pub const LEGACY_WRITERS: &[&str] = &[
    "automation.alarm_scheduler",
    "automation.schedules_alarm_dispatcher",
    "automation.schedules_reminder_dispatcher",
    "automation.schedules_timer_finished",
];

const OK_MESSAGE: &str =
    "preflight ok: retired writers absent/off, no ringing alarm, scheduler ready";

/// One preflight run.
pub struct Options<'a> {
    /// Home Assistant base URL.
    pub ha_url: &'a str,
    /// File with the HA token.
    pub ha_token_file: &'a str,
    /// Scheduler base URL.
    pub scheduler_url: &'a str,
    /// File with the scheduler token.
    pub scheduler_token_file: &'a str,
    /// Expected `input_text.scheduler_endpoint` value.
    pub expected_endpoint: &'a str,
}

/// Returns failures for writers that could still execute schedules.
#[must_use]
pub fn legacy_writer_failures(states: &std::collections::BTreeMap<String, String>) -> Vec<String> {
    let mut failures = Vec::new();
    for entity in LEGACY_WRITERS {
        match states.get(*entity) {
            None => {}
            Some(state) if state == "off" => {}
            Some(state) => failures.push(format!(
                "legacy writer {entity} is '{state}', expected absent or 'off'"
            )),
        }
    }
    failures
}

/// Executes every check and returns the process exit status.
pub fn run(options: &Options, stdout: &mut dyn Write) -> i32 {
    let client = Client::new(PREFLIGHT_TIMEOUT);
    let ha_token = read_token(options.ha_token_file);
    let scheduler_token = read_token(options.scheduler_token_file);

    let ha = client.get(
        &format!("{}/api/states", trim_right(options.ha_url)),
        &ha_token,
    );
    let states = if ha.status == 200 {
        ha.value.and_then(|value| value.as_array().cloned())
    } else {
        None
    };
    let Some(list) = states else {
        let _ = writeln!(stdout, "FAIL: cannot read HA states (HTTP {})", ha.status);
        return 1;
    };
    let mut by_id = std::collections::BTreeMap::new();
    for item in &list {
        let Some(entity) = item.as_object() else {
            continue;
        };
        let id = entity.get("entity_id").and_then(|v| v.as_str());
        let value = entity.get("state").and_then(|v| v.as_str());
        if let Some(id) = id {
            by_id.insert(id.to_string(), value.unwrap_or("").to_string());
        }
    }

    let mut failures = legacy_writer_failures(&by_id);

    if by_id.get("input_boolean.alarm_ringing").map(String::as_str) == Some("on") {
        failures
            .push("an alarm is ringing in HA; stop it before starting the executor".to_string());
    }

    if !options.expected_endpoint.is_empty() {
        match by_id.get("input_text.scheduler_endpoint") {
            Some(actual) if actual == options.expected_endpoint => {}
            Some(actual) => failures.push(format!(
                "input_text.scheduler_endpoint is '{actual}', expected '{}'",
                options.expected_endpoint
            )),
            None => failures.push(format!(
                "input_text.scheduler_endpoint is None, expected '{}'",
                options.expected_endpoint
            )),
        }
    }

    let status = client.post_json(
        &format!(
            "{}/home.v1.SchedulerService/GetStatus",
            trim_right(options.scheduler_url)
        ),
        &scheduler_token,
        "{}",
    );
    if status.status != 200 {
        failures.push(format!(
            "scheduler GetStatus failed (HTTP {})",
            status.status
        ));
    } else if !status
        .value
        .as_ref()
        .and_then(|value| value.get("ready"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        failures.push("scheduler reports ready=false".to_string());
    }

    if !failures.is_empty() {
        for failure in &failures {
            let _ = writeln!(stdout, "FAIL: {failure}");
        }
        return 1;
    }

    let _ = writeln!(stdout, "{OK_MESSAGE}");
    0
}

fn read_token(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn trim_right(url: &str) -> &str {
    url.trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn states(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn absent_legacy_writers_are_safe() {
        assert!(legacy_writer_failures(&states(&[])).is_empty());
    }

    #[test]
    fn off_legacy_writers_are_safe() {
        let values = states(&[
            ("automation.alarm_scheduler", "off"),
            ("automation.schedules_alarm_dispatcher", "off"),
            ("automation.schedules_reminder_dispatcher", "off"),
            ("automation.schedules_timer_finished", "off"),
        ]);
        assert!(legacy_writer_failures(&values).is_empty());
    }

    #[test]
    fn on_unknown_and_unavailable_writers_fail() {
        let values = states(&[
            ("automation.alarm_scheduler", "on"),
            ("automation.schedules_alarm_dispatcher", "unknown"),
            ("automation.schedules_reminder_dispatcher", "unavailable"),
        ]);
        let failures = legacy_writer_failures(&values);
        assert_eq!(failures.len(), 3);
        assert!(failures[0].contains("is 'on'"));
        assert!(failures[1].contains("is 'unknown'"));
        assert!(failures[2].contains("is 'unavailable'"));
    }
}

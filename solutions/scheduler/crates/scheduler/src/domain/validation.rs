//! Input validation shared by the future store and API layers.
//!
//! Messages are the Go store's exact strings: they are part of the observable
//! contract because clients and golden fixtures assert on them.

use std::fmt;
use std::fmt::Write as _;

use crate::domain::rule::{Repeat, RuleError};
use crate::domain::schedule::ScheduleKind;

/// Transport-neutral classification of a domain error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    InvalidArgument,
    NotFound,
    Conflict,
    Idempotency,
    Precondition,
    LimitExceeded,
    ConsumerDenied,
}

impl ErrorKind {
    /// Go error prefix for this kind.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid argument",
            Self::NotFound => "not found",
            Self::Conflict => "revision conflict",
            Self::Idempotency => "idempotency key conflict",
            Self::Precondition => "precondition failed",
            Self::LimitExceeded => "limit exceeded",
            Self::ConsumerDenied => "consumer not allowed",
        }
    }
}

/// A domain failure with the Go-compatible rendered message
/// (`<kind>: <detail>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainError {
    kind: ErrorKind,
    detail: String,
}

impl DomainError {
    /// Builds an error with an explicit kind and detail.
    #[must_use]
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// `invalid argument: <detail>`.
    #[must_use]
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidArgument, detail)
    }

    /// `invalid argument: invalid schedule rule: <rule detail>`.
    #[must_use]
    pub fn invalid_rule(error: &RuleError) -> Self {
        Self::invalid(error.to_string())
    }

    /// `not found: <detail>`.
    #[must_use]
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, detail)
    }

    /// `precondition failed: <detail>`.
    #[must_use]
    pub fn precondition(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::Precondition, detail)
    }

    /// `limit exceeded: <detail>`.
    #[must_use]
    pub fn limit_exceeded(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::LimitExceeded, detail)
    }

    /// Error kind for transport mapping.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Detail without the kind prefix.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind.prefix(), self.detail)
    }
}

impl std::error::Error for DomainError {}

/// Label length limit shared by schedules and timers (bytes).
pub const MAX_LABEL_BYTES: usize = 200;

/// Idempotency key length limit (bytes).
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 200;

/// Ack detail length limit (bytes).
pub const MAX_DETAIL_BYTES: usize = 1024;

/// Maximum generic task delay: 10 * 365 days, like the Go constant.
pub const MAX_TASK_DELAY_SECONDS: i64 = 10 * 365 * 24 * 60 * 60;

/// Validates a schedule or timer label.
pub fn validate_label(label: &str) -> Result<(), DomainError> {
    if label.len() > MAX_LABEL_BYTES {
        return Err(DomainError::invalid(format!(
            "label exceeds {MAX_LABEL_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Validates a required schedule or timer id.
pub fn validate_id(id: &str) -> Result<(), DomainError> {
    if id.trim().is_empty() {
        return Err(DomainError::invalid("id is required"));
    }
    Ok(())
}

/// Validates a required event id.
pub fn validate_event_id(event_id: &str) -> Result<(), DomainError> {
    if event_id.trim().is_empty() {
        return Err(DomainError::invalid("event_id is required"));
    }
    Ok(())
}

/// Validates an optional idempotency key.
pub fn validate_idempotency_key(key: &str) -> Result<(), DomainError> {
    if key.len() > MAX_IDEMPOTENCY_KEY_BYTES {
        return Err(DomainError::invalid(format!(
            "idempotency key exceeds {MAX_IDEMPOTENCY_KEY_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Validates a JSON payload and its size.
pub fn validate_payload(payload_json: &str, max_payload_bytes: usize) -> Result<(), DomainError> {
    if payload_json.len() > max_payload_bytes {
        return Err(DomainError::invalid(format!(
            "payload exceeds {max_payload_bytes} bytes"
        )));
    }
    if serde_json::from_str::<serde_json::Value>(payload_json).is_err() {
        return Err(DomainError::invalid("payload_json must be valid JSON"));
    }
    Ok(())
}

/// Validates a generic task type: `^[a-z][a-z0-9._-]{0,63}$`.
pub fn validate_task_type(task_type: &str) -> Result<(), DomainError> {
    let mut chars = task_type.chars();
    let valid = chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
        && task_type.len() <= 64;
    if !valid {
        return Err(DomainError::invalid(
            "task_type must be 1..64 chars of [a-z0-9._-] starting with a letter",
        ));
    }
    Ok(())
}

/// Validates a task queue: `^[a-z][a-z0-9._-]{0,31}$`.
pub fn validate_queue(queue: &str) -> Result<(), DomainError> {
    let mut chars = queue.chars();
    let valid = chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
        && queue.len() <= 32;
    if !valid {
        return Err(DomainError::invalid(
            "queue must be 1..32 chars of [a-z0-9._-] starting with a letter",
        ));
    }
    Ok(())
}

/// Validates a timer duration against the configured maximum.
pub fn validate_duration(duration_seconds: i64, max_seconds: i64) -> Result<(), DomainError> {
    if duration_seconds <= 0 {
        return Err(DomainError::invalid("duration_seconds must be positive"));
    }
    if duration_seconds > max_seconds {
        return Err(DomainError::invalid(format!(
            "duration exceeds {}",
            format_go_duration(max_seconds)
        )));
    }
    Ok(())
}

/// Validates a requested task run time and returns the effective due second.
///
/// Like Go, a zero or past `run_at_unix` means "now"; the maximum delay is
/// measured from `now_unix` on the effective due time.
pub fn validate_run_at(
    run_at_unix: i64,
    now_unix: i64,
    max_delay_seconds: i64,
) -> Result<i64, DomainError> {
    if run_at_unix < 0 {
        return Err(DomainError::invalid("run_at_unix must not be negative"));
    }
    let due = if run_at_unix == 0 || run_at_unix < now_unix {
        now_unix
    } else {
        run_at_unix
    };
    if due > now_unix.saturating_add(max_delay_seconds) {
        return Err(DomainError::invalid(format!(
            "run_at_unix is more than {} ahead",
            format_go_duration(max_delay_seconds)
        )));
    }
    Ok(due)
}

/// Validates an ack detail string.
pub fn validate_detail(detail: &str) -> Result<(), DomainError> {
    if detail.len() > MAX_DETAIL_BYTES {
        return Err(DomainError::invalid(format!(
            "detail exceeds {MAX_DETAIL_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Validates a schedule kind.
pub fn validate_schedule_kind(kind: ScheduleKind) -> Result<(), DomainError> {
    match kind {
        ScheduleKind::Alarm | ScheduleKind::Reminder | ScheduleKind::Scenario => Ok(()),
        ScheduleKind::Unspecified => Err(DomainError::invalid("unsupported schedule kind")),
    }
}

/// Reminders are one-shot only.
pub fn validate_reminder_repeat(kind: ScheduleKind, repeat: Repeat) -> Result<(), DomainError> {
    if kind == ScheduleKind::Reminder && repeat != Repeat::Once {
        return Err(DomainError::invalid("reminders must use REPEAT_KIND_ONCE"));
    }
    Ok(())
}

/// Renders a whole number of seconds the way Go's `time.Duration.String` does.
#[must_use]
pub fn format_go_duration(total_seconds: i64) -> String {
    if total_seconds == 0 {
        return "0s".to_owned();
    }
    let sign = if total_seconds < 0 { "-" } else { "" };
    let total = total_seconds.unsigned_abs();
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    let mut out = String::with_capacity(16);
    out.push_str(sign);
    if hours > 0 {
        write!(out, "{hours}h").expect("write to string");
    }
    if hours > 0 || minutes > 0 {
        write!(out, "{minutes}m").expect("write to string");
    }
    write!(out, "{seconds}s").expect("write to string");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_duration_formatting_matches_time_duration() {
        assert_eq!(format_go_duration(0), "0s");
        assert_eq!(format_go_duration(1), "1s");
        assert_eq!(format_go_duration(60), "1m0s");
        assert_eq!(format_go_duration(90), "1m30s");
        assert_eq!(format_go_duration(3_600), "1h0m0s");
        assert_eq!(format_go_duration(3_661), "1h1m1s");
        assert_eq!(format_go_duration(86_400), "24h0m0s");
        assert_eq!(format_go_duration(MAX_TASK_DELAY_SECONDS), "87600h0m0s");
    }

    #[test]
    fn messages_match_go_text() {
        assert_eq!(
            DomainError::invalid_rule(&RuleError::new("hour 25 outside 0..23")).to_string(),
            "invalid argument: invalid schedule rule: hour 25 outside 0..23"
        );
        assert_eq!(
            validate_task_type("Bad")
                .expect_err("uppercase")
                .to_string(),
            "invalid argument: task_type must be 1..64 chars of [a-z0-9._-] starting with a letter"
        );
        assert_eq!(
            validate_label(&"x".repeat(201))
                .expect_err("too long")
                .to_string(),
            "invalid argument: label exceeds 200 bytes"
        );
        assert_eq!(
            validate_detail(&"x".repeat(1025))
                .expect_err("too long")
                .to_string(),
            "invalid argument: detail exceeds 1024 bytes"
        );
        assert_eq!(
            validate_duration(86_401, 86_400)
                .expect_err("too long")
                .to_string(),
            "invalid argument: duration exceeds 24h0m0s"
        );
        assert_eq!(
            validate_run_at(MAX_TASK_DELAY_SECONDS + 2, 1, MAX_TASK_DELAY_SECONDS)
                .expect_err("too far")
                .to_string(),
            "invalid argument: run_at_unix is more than 87600h0m0s ahead"
        );
    }

    #[test]
    fn task_type_and_queue_rules() {
        assert!(validate_task_type("notify.push").is_ok());
        assert!(validate_task_type("a").is_ok());
        assert!(validate_task_type("1abc").is_err());
        assert!(validate_task_type("").is_err());
        assert!(validate_task_type(&"a".repeat(65)).is_err());
        assert!(validate_queue("voice-work").is_ok());
        assert!(validate_queue(&"a".repeat(33)).is_err());
    }

    #[test]
    fn run_at_clamps_and_returns_due() {
        assert_eq!(validate_run_at(0, 100, 10).expect("due"), 100);
        assert_eq!(validate_run_at(50, 100, 10).expect("due"), 100);
        assert_eq!(validate_run_at(105, 100, 10).expect("due"), 105);
        assert!(validate_run_at(-1, 100, 10).is_err());
    }
}

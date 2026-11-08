//! Stable localization keys for scheduler responses and errors.
//!
//! Keys are namespaced snake case and are the contract the Android app resolves
//! through `strings.xml`. The legacy human-readable message stays on the wire
//! for older clients; it is never the primary text for a current client.

use home_proto::{localized, localized_with, v1::LocalizedText};

use crate::domain::ErrorKind;
use crate::store::StoreError;

/// Attaches a localization detail to an error built directly by an API handler.
pub fn connect(error: connectrpc::ConnectError, key: &str) -> connectrpc::ConnectError {
    error.with_detail(home_proto::error_detail(&localized(key)))
}

/// Key for an error that has no more specific mapping.
fn kind_key(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::InvalidArgument => "scheduler.error.invalid_argument",
        ErrorKind::NotFound => "scheduler.error.not_found",
        ErrorKind::Conflict => "scheduler.error.revision_conflict",
        ErrorKind::Idempotency => "scheduler.error.idempotency_conflict",
        ErrorKind::Precondition => "scheduler.error.precondition",
        ErrorKind::LimitExceeded => "scheduler.error.limit_exceeded",
        ErrorKind::ConsumerDenied => "scheduler.error.consumer_denied",
    }
}

/// Localized text for a store failure.
///
/// Exact details whose wording is part of the client experience get a specific
/// key; dynamic bounds carry typed arguments; everything else falls back to a
/// generic per-kind key so a raw English detail never reaches the app.
#[must_use]
pub fn error_text(error: &StoreError) -> LocalizedText {
    match error {
        StoreError::NotFound => localized("scheduler.error.not_found"),
        StoreError::ConsumerDenied => localized("scheduler.error.consumer_denied"),
        StoreError::Sqlite(_) => localized("scheduler.error.internal"),
        StoreError::Domain(domain) => {
            let detail = domain.detail();
            detail_key(detail).unwrap_or_else(|| localized(kind_key(domain.kind())))
        }
    }
}

fn detail_key(detail: &str) -> Option<LocalizedText> {
    let text = match detail {
        "id is required" => localized("scheduler.error.id_required"),
        "event_id is required" => localized("scheduler.error.event_id_required"),
        "label is required" => localized("scheduler.error.label_required"),
        "duration_seconds must be positive" => localized("scheduler.error.duration_positive"),
        "schedule is not ringing" => localized("scheduler.error.schedule_not_ringing"),
        "event is not leased" => localized("scheduler.error.event_not_leased"),
        "lease has expired" => localized("scheduler.error.lease_expired"),
        "idempotency key already exists" => localized("scheduler.error.idempotency_key_exists"),
        "idempotency key reused with a different request" => {
            localized("scheduler.error.idempotency_key_reused")
        }
        "idempotent timer no longer exists" => {
            localized("scheduler.error.idempotent_timer_missing")
        }
        "idempotent schedule no longer exists" => {
            localized("scheduler.error.idempotent_schedule_missing")
        }
        "schedule has no upcoming occurrence" => {
            localized("scheduler.error.no_upcoming_occurrence")
        }
        "unsupported schedule kind" => localized("scheduler.error.unsupported_schedule_kind"),
        "reminders must use REPEAT_KIND_ONCE" => localized("scheduler.error.reminder_repeat"),
        "payload_json must be valid JSON" => localized("scheduler.error.invalid_json"),
        "scenario body is not serializable" => {
            localized("scheduler.error.scenario_not_serializable")
        }
        "empty dedupe key" => localized("scheduler.error.empty_dedupe_key"),
        "limit must be 1..200" => localized("scheduler.error.limit_range"),
        "snooze minutes must be 0..1440" => localized("scheduler.error.snooze_range"),
        "alarm not found" => localized("scheduler.error.alarm_not_found"),
        "event is leased by another consumer" => localized("scheduler.error.event_leased_by_other"),
        "only task events can be cancelled; use StopSchedule" => {
            localized("scheduler.error.task_cancel_only")
        }
        "task is already leased by an executor" => localized("scheduler.error.task_leased"),
        _ => return detail_key_with_args(detail),
    };
    Some(text)
}

fn detail_key_with_args(detail: &str) -> Option<LocalizedText> {
    if let Some(value) = detail.strip_prefix("label exceeds ") {
        return Some(localized_with(
            "scheduler.error.label_too_long",
            [("limit", bytes(value))],
        ));
    }
    if let Some(value) = detail.strip_prefix("idempotency key exceeds ") {
        return Some(localized_with(
            "scheduler.error.idempotency_key_too_long",
            [("limit", bytes(value))],
        ));
    }
    if let Some(value) = detail.strip_prefix("payload exceeds ") {
        return Some(localized_with(
            "scheduler.error.payload_too_large",
            [("limit", bytes(value))],
        ));
    }
    if let Some(value) = detail.strip_prefix("detail exceeds ") {
        return Some(localized_with(
            "scheduler.error.detail_too_long",
            [("limit", bytes(value))],
        ));
    }
    if let Some(value) = detail.strip_prefix("duration exceeds ") {
        return Some(localized_with(
            "scheduler.error.duration_too_long",
            [("limit", value.to_owned())],
        ));
    }
    if let Some(value) = detail.strip_prefix("run_at_unix is more than ") {
        let limit = value.strip_suffix(" ahead").unwrap_or(value);
        return Some(localized_with(
            "scheduler.error.run_at_too_far",
            [("limit", limit.to_owned())],
        ));
    }
    if let Some(value) = detail.strip_prefix("at most ") {
        let limit = value.strip_suffix(" active timers").unwrap_or(value);
        return Some(localized_with(
            "scheduler.error.timer_limit",
            [("limit", limit.to_owned())],
        ));
    }
    if let Some(owner) = detail.strip_prefix("managed by scenario ") {
        return Some(localized_with(
            "scheduler.error.scenario_managed",
            [("scenario", owner.to_owned())],
        ));
    }
    if detail.starts_with("invalid schedule rule: ") {
        return Some(localized("scheduler.error.invalid_schedule_rule"));
    }
    if let Some(state) = detail.strip_prefix("timer is ") {
        return Some(localized_with(
            "scheduler.error.timer_not_active",
            [("state", state.to_owned())],
        ));
    }
    None
}

fn bytes(value: &str) -> String {
    value.strip_suffix(" bytes").unwrap_or(value).to_owned()
}

/// Localized reason of a durable event. Unknown executor details stay
/// untranslated (empty key) because they are free-form executor text.
#[must_use]
pub fn event_detail_text(detail: &str) -> LocalizedText {
    match detail {
        "deleted" => localized("scheduler.event.deleted"),
        "stopped" => localized("scheduler.event.stopped"),
        "skipped" => localized("scheduler.event.skipped"),
        "snoozed" => localized("scheduler.event.snoozed"),
        "cancelled" => localized("scheduler.event.cancelled"),
        "ring_window_expired" => localized("scheduler.event.ring_window_expired"),
        "rule_changed" => localized("scheduler.event.rule_changed"),
        _ => detail_with_args(detail),
    }
}

fn detail_with_args(detail: &str) -> LocalizedText {
    if let Some(value) = detail.strip_prefix("snoozed for ") {
        let minutes = value.strip_suffix(" minutes").unwrap_or(value);
        return localized_with(
            "scheduler.event.snoozed_for",
            [("minutes", minutes.to_owned())],
        );
    }
    if let Some(value) = detail.strip_prefix("missed_grace: ") {
        let seconds = value.strip_suffix(" seconds late").unwrap_or(value);
        return localized_with("scheduler.event.missed", [("seconds", seconds.to_owned())]);
    }
    localized("")
}

/// Localized last-tick failure for `GetStatus`.
#[must_use]
pub fn tick_error_text(error: &str) -> LocalizedText {
    if error.trim().is_empty() {
        return localized("");
    }
    localized("scheduler.status.tick_failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DomainError;

    #[test]
    fn known_details_get_specific_keys_and_dynamic_bounds_keep_arguments() {
        let not_ringing = error_text(&StoreError::precondition("schedule is not ringing"));
        assert_eq!(not_ringing.key, "scheduler.error.schedule_not_ringing");

        let too_long = error_text(&StoreError::invalid("label exceeds 200 bytes"));
        assert_eq!(too_long.key, "scheduler.error.label_too_long");
        assert_eq!(too_long.args["limit"], "200");

        let timer_limit = StoreError::limit("at most 64 active timers");
        let text = error_text(&timer_limit);
        assert_eq!(text.key, "scheduler.error.timer_limit");
        assert_eq!(text.args["limit"], "64");
    }

    #[test]
    fn unknown_details_never_use_the_legacy_sentence() {
        let error = StoreError::Domain(DomainError::invalid("an internal-shaped detail"));
        assert_eq!(error_text(&error).key, "scheduler.error.invalid_argument");
        assert!(error_text(&error).args.is_empty());
        assert_eq!(
            error_text(&StoreError::Sqlite(home_sqlite::SqliteError::Panicked)).key,
            "scheduler.error.internal"
        );
    }

    #[test]
    fn event_details_are_keys_with_typed_arguments() {
        assert_eq!(event_detail_text("deleted").key, "scheduler.event.deleted");
        let snoozed = event_detail_text("snoozed for 9 minutes");
        assert_eq!(snoozed.key, "scheduler.event.snoozed_for");
        assert_eq!(snoozed.args["minutes"], "9");
        let missed = event_detail_text("missed_grace: 601 seconds late");
        assert_eq!(missed.key, "scheduler.event.missed");
        assert_eq!(missed.args["seconds"], "601");
        assert!(event_detail_text("executor said something").key.is_empty());
    }
}

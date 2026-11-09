//! Stable localization keys for intercom errors.

use home_proto::{localized, v1::LocalizedText};

use crate::domain::ErrorKind;

/// Key for an error message with no more specific mapping.
pub fn kind_key(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::InvalidArgument => "intercom.error.invalid_argument",
        ErrorKind::NotFound => "intercom.error.not_found",
        ErrorKind::AlreadyExists => "intercom.error.already_exists",
        ErrorKind::FailedPrecondition => "intercom.error.failed_precondition",
        ErrorKind::PermissionDenied => "intercom.error.permission_denied",
        ErrorKind::Unavailable => "intercom.error.unavailable",
    }
}

/// Localized text for a domain error.
#[must_use]
pub fn error_text(kind: ErrorKind, message: &str) -> LocalizedText {
    localized(message_key(message).unwrap_or_else(|| kind_key(kind)))
}

fn message_key(message: &str) -> Option<&'static str> {
    let key = match message {
        "unknown endpoint" => "intercom.error.unknown_endpoint",
        "endpoint kind does not match the token" => "intercom.error.endpoint_kind_mismatch",
        "callee_endpoint_id is required" => "intercom.error.callee_required",
        "caller and callee are the same endpoint" => "intercom.error.same_endpoint",
        "idempotency conflict" => "intercom.error.idempotency_conflict",
        "idempotency key belongs to another call" => "intercom.error.idempotency_other_call",
        "unknown callee" => "intercom.error.unknown_callee",
        "callee is offline" => "intercom.error.callee_offline",
        "busy" => "intercom.error.busy",
        "only the callee can answer" => "intercom.error.only_callee_can_answer",
        "call is not ringing" => "intercom.error.call_not_ringing",
        "unknown call" => "intercom.error.unknown_call",
        "not a participant of this call" => "intercom.error.not_participant",
        "call has ended" => "intercom.error.call_ended",
        "ICE candidate is too large" => "intercom.error.ice_too_large",
        "only the caller can send an offer" => "intercom.error.only_caller_can_offer",
        "offer already sent" => "intercom.error.offer_already_sent",
        "only the callee can send an answer" => "intercom.error.only_callee_can_answer_sdp",
        "answer before the call was answered" => "intercom.error.answer_before_answered",
        "answer already sent" => "intercom.error.answer_already_sent",
        "SDP body is too large" => "intercom.error.sdp_too_large",
        "signal is required" => "intercom.error.signal_required",
        _ => return None,
    };
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_messages_map_and_unknown_falls_back_to_the_kind() {
        assert_eq!(
            error_text(ErrorKind::Unavailable, "busy").key,
            "intercom.error.busy"
        );
        assert_eq!(
            error_text(ErrorKind::PermissionDenied, "executor said something").key,
            "intercom.error.permission_denied"
        );
    }
}

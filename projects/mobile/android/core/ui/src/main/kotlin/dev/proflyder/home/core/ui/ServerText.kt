package dev.proflyder.home.core.ui

import android.content.Context
import androidx.annotation.StringRes

/**
 * One server localization key: the resource that renders it and the order of
 * the typed arguments the template expects (`%1$s`, `%2$s`, ...).
 */
private data class ServerString(@StringRes val res: Int, val args: List<String> = emptyList())

private fun text(@StringRes res: Int, vararg args: String) = ServerString(res, args.toList())

/**
 * Resolves a server `LocalizedText.key` to a local resource.
 *
 * Unknown keys return null so the caller can fall back to the server's legacy
 * text and then to a generic message. The map is data, not branching: adding a
 * server key is one line plus its `strings.xml` entries.
 */
fun serverText(context: Context, key: String, args: Map<String, String>): String? {
    val entry = SERVER_STRINGS[key] ?: return null
    val values = entry.args.map { name -> args[name].orEmpty() }.toTypedArray()
    return context.getString(entry.res, *values)
}

private val SERVER_STRINGS: Map<String, ServerString> = mapOf(
    // Scheduler errors.
    "scheduler.error.not_found" to text(R.string.server_scheduler_not_found),
    "scheduler.error.invalid_argument" to text(R.string.server_scheduler_invalid_argument),
    "scheduler.error.revision_conflict" to text(R.string.server_scheduler_revision_conflict),
    "scheduler.error.idempotency_conflict" to text(R.string.server_scheduler_idempotency_conflict),
    "scheduler.error.precondition" to text(R.string.server_scheduler_precondition),
    "scheduler.error.limit_exceeded" to text(R.string.server_scheduler_limit_exceeded),
    "scheduler.error.consumer_denied" to text(R.string.server_scheduler_consumer_denied),
    "scheduler.error.internal" to text(R.string.server_scheduler_internal),
    "scheduler.error.id_required" to text(R.string.server_scheduler_id_required),
    "scheduler.error.event_id_required" to text(R.string.server_scheduler_event_id_required),
    "scheduler.error.label_required" to text(R.string.server_scheduler_label_required),
    "scheduler.error.duration_positive" to text(R.string.server_scheduler_duration_positive),
    "scheduler.error.schedule_not_ringing" to text(R.string.server_scheduler_schedule_not_ringing),
    "scheduler.error.event_not_leased" to text(R.string.server_scheduler_event_not_leased),
    "scheduler.error.lease_expired" to text(R.string.server_scheduler_lease_expired),
    "scheduler.error.idempotency_key_exists" to text(R.string.server_scheduler_idempotency_key_exists),
    "scheduler.error.idempotency_key_reused" to text(R.string.server_scheduler_idempotency_key_reused),
    "scheduler.error.idempotent_timer_missing" to text(R.string.server_scheduler_idempotent_timer_missing),
    "scheduler.error.idempotent_schedule_missing" to text(R.string.server_scheduler_idempotent_schedule_missing),
    "scheduler.error.no_upcoming_occurrence" to text(R.string.server_scheduler_no_upcoming_occurrence),
    "scheduler.error.alarm_not_found" to text(R.string.server_scheduler_alarm_not_found),
    "scheduler.error.event_leased_by_other" to text(R.string.server_scheduler_event_leased_by_other),
    "scheduler.error.task_cancel_only" to text(R.string.server_scheduler_task_cancel_only),
    "scheduler.error.task_leased" to text(R.string.server_scheduler_task_leased),
    "scheduler.error.timer_not_active" to text(R.string.server_scheduler_timer_not_active, "state"),
    "scheduler.error.unsupported_schedule_kind" to text(R.string.server_scheduler_unsupported_kind),
    "scheduler.error.reminder_repeat" to text(R.string.server_scheduler_reminder_repeat),
    "scheduler.error.invalid_json" to text(R.string.server_scheduler_invalid_json),
    "scheduler.error.scenario_not_serializable" to text(R.string.server_scheduler_scenario_not_serializable),
    "scheduler.error.empty_dedupe_key" to text(R.string.server_scheduler_empty_dedupe_key),
    "scheduler.error.limit_range" to text(R.string.server_scheduler_limit_range),
    "scheduler.error.snooze_range" to text(R.string.server_scheduler_snooze_range),
    "scheduler.error.label_too_long" to text(R.string.server_scheduler_label_too_long, "limit"),
    "scheduler.error.idempotency_key_too_long" to
        text(R.string.server_scheduler_idempotency_key_too_long, "limit"),
    "scheduler.error.payload_too_large" to text(R.string.server_scheduler_payload_too_large, "limit"),
    "scheduler.error.detail_too_long" to text(R.string.server_scheduler_detail_too_long, "limit"),
    "scheduler.error.duration_too_long" to text(R.string.server_scheduler_duration_too_long, "limit"),
    "scheduler.error.run_at_too_far" to text(R.string.server_scheduler_run_at_too_far, "limit"),
    "scheduler.error.timer_limit" to text(R.string.server_scheduler_timer_limit, "limit"),
    "scheduler.error.scenario_managed" to text(R.string.server_scheduler_scenario_managed, "scenario"),
    "scheduler.error.invalid_schedule_rule" to text(R.string.server_scheduler_invalid_rule),
    "scheduler.error.unsupported_repeat" to text(R.string.server_scheduler_unsupported_repeat),
    "scheduler.error.alarm_id_required" to text(R.string.server_scheduler_alarm_id_required),
    "scheduler.error.schedule_required" to text(R.string.server_scheduler_schedule_required),
    "scheduler.error.scenario_required" to text(R.string.server_scheduler_scenario_required),
    "scheduler.error.expected_revision_negative" to text(R.string.server_scheduler_expected_revision),
    "scheduler.error.unsupported_scenario_part" to text(R.string.server_scheduler_unsupported_scenario_part),

    // Scheduler event details.
    "scheduler.event.deleted" to text(R.string.server_event_deleted),
    "scheduler.event.stopped" to text(R.string.server_event_stopped),
    "scheduler.event.skipped" to text(R.string.server_event_skipped),
    "scheduler.event.snoozed" to text(R.string.server_event_snoozed),
    "scheduler.event.cancelled" to text(R.string.server_event_cancelled),
    "scheduler.event.ring_window_expired" to text(R.string.server_event_ring_window_expired),
    "scheduler.event.rule_changed" to text(R.string.server_event_rule_changed),
    "scheduler.event.snoozed_for" to text(R.string.server_event_snoozed_for, "minutes"),
    "scheduler.event.missed" to text(R.string.server_event_missed, "seconds"),
    "scheduler.status.tick_failed" to text(R.string.server_status_tick_failed),

    // Intercom errors.
    "intercom.error.invalid_argument" to text(R.string.server_intercom_invalid_argument),
    "intercom.error.not_found" to text(R.string.server_intercom_not_found),
    "intercom.error.already_exists" to text(R.string.server_intercom_already_exists),
    "intercom.error.failed_precondition" to text(R.string.server_intercom_failed_precondition),
    "intercom.error.permission_denied" to text(R.string.server_intercom_permission_denied),
    "intercom.error.unavailable" to text(R.string.server_intercom_unavailable),
    "intercom.error.unknown_endpoint" to text(R.string.server_intercom_unknown_endpoint),
    "intercom.error.endpoint_kind_mismatch" to text(R.string.server_intercom_endpoint_kind_mismatch),
    "intercom.error.endpoint_token_mismatch" to text(R.string.server_intercom_endpoint_token_mismatch),
    "intercom.error.callee_required" to text(R.string.server_intercom_callee_required),
    "intercom.error.same_endpoint" to text(R.string.server_intercom_same_endpoint),
    "intercom.error.idempotency_conflict" to text(R.string.server_intercom_idempotency_conflict),
    "intercom.error.idempotency_other_call" to text(R.string.server_intercom_idempotency_other_call),
    "intercom.error.unknown_callee" to text(R.string.server_intercom_unknown_callee),
    "intercom.error.callee_offline" to text(R.string.server_intercom_callee_offline),
    "intercom.error.busy" to text(R.string.server_intercom_busy),
    "intercom.error.only_callee_can_answer" to text(R.string.server_intercom_only_callee_can_answer),
    "intercom.error.call_not_ringing" to text(R.string.server_intercom_call_not_ringing),
    "intercom.error.unknown_call" to text(R.string.server_intercom_unknown_call),
    "intercom.error.not_participant" to text(R.string.server_intercom_not_participant),
    "intercom.error.call_ended" to text(R.string.server_intercom_call_ended),
    "intercom.error.ice_too_large" to text(R.string.server_intercom_ice_too_large),
    "intercom.error.only_caller_can_offer" to text(R.string.server_intercom_only_caller_can_offer),
    "intercom.error.offer_already_sent" to text(R.string.server_intercom_offer_already_sent),
    "intercom.error.only_callee_can_answer_sdp" to text(R.string.server_intercom_only_callee_can_answer_sdp),
    "intercom.error.answer_before_answered" to text(R.string.server_intercom_answer_before_answered),
    "intercom.error.answer_already_sent" to text(R.string.server_intercom_answer_already_sent),
    "intercom.error.sdp_too_large" to text(R.string.server_intercom_sdp_too_large),
    "intercom.error.signal_required" to text(R.string.server_intercom_signal_required),

    // Control catalog.
    "catalog.action.speak_title" to text(R.string.server_catalog_speak_title),
)

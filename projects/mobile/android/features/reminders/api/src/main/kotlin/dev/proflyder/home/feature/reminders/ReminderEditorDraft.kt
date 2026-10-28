package dev.proflyder.home.feature.reminders

import java.time.LocalDate

/** Non-secret reminder editor draft persisted in SavedStateHandle. */
data class ReminderEditorDraft(
    val existingId: String? = null,
    val existingRevision: Long? = null,
    val label: String = "",
    val date: LocalDate = LocalDate.now(),
    val hour: Int = 9,
    val minute: Int = 0,
    val timezone: String = java.time.ZoneId.systemDefault().id,
    val enabled: Boolean = true,
)

object ReminderEditorDraftCodec {
    private const val KEY_EXISTING_ID = "reminder_existing_id"
    private const val KEY_EXISTING_REVISION = "reminder_existing_revision"
    private const val KEY_LABEL = "reminder_label"
    private const val KEY_DATE = "reminder_date"
    private const val KEY_HOUR = "reminder_hour"
    private const val KEY_MINUTE = "reminder_minute"
    private const val KEY_TIMEZONE = "reminder_timezone"
    private const val KEY_ENABLED = "reminder_enabled"

    fun toMap(draft: ReminderEditorDraft): Map<String, Any?> = mapOf(
        KEY_EXISTING_ID to draft.existingId,
        KEY_EXISTING_REVISION to draft.existingRevision,
        KEY_LABEL to draft.label,
        KEY_DATE to draft.date.toString(),
        KEY_HOUR to draft.hour,
        KEY_MINUTE to draft.minute,
        KEY_TIMEZONE to draft.timezone,
        KEY_ENABLED to draft.enabled,
    )

    fun fromMap(values: Map<String, Any?>): ReminderEditorDraft? {
        if (values.isEmpty()) return null
        val hour = (values[KEY_HOUR] as? Number)?.toInt() ?: return null
        val minute = (values[KEY_MINUTE] as? Number)?.toInt() ?: return null
        if (hour !in 0..23 || minute !in 0..59) return null
        val timezone = values[KEY_TIMEZONE] as? String ?: return null
        if (timezone.isBlank()) return null
        val date = (values[KEY_DATE] as? String)
            ?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
            ?: return null
        return ReminderEditorDraft(
            existingId = values[KEY_EXISTING_ID] as? String,
            existingRevision = (values[KEY_EXISTING_REVISION] as? Number)?.toLong(),
            label = values[KEY_LABEL] as? String ?: "",
            date = date,
            hour = hour,
            minute = minute,
            timezone = timezone,
            enabled = values[KEY_ENABLED] as? Boolean ?: true,
        )
    }
}

enum class ReminderEditorCompletion {
    SAVED,
    DELETED,
}

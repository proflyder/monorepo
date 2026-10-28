package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import java.time.LocalDate

/**
 * Non-secret editor draft persisted in SavedStateHandle. It intentionally holds
 * no token, endpoint credentials, or schedule payload.
 */
data class AlarmEditorDraft(
    val existingId: String? = null,
    val existingRevision: Long? = null,
    val label: String = "",
    val hour: Int = 7,
    val minute: Int = 0,
    val timezone: String = java.time.ZoneId.systemDefault().id,
    val repeat: RepeatKind = RepeatKind.DAILY,
    val weekdays: List<Int> = emptyList(),
    val enabled: Boolean = true,
    val onceDate: LocalDate? = null,
)

/**
 * Maps a draft to Bundle-compatible primitive values. Kept Android-free so the
 * round-trip and malformed-input behavior can be unit-tested on the JVM.
 */
object AlarmEditorDraftCodec {
    private const val KEY_EXISTING_ID = "alarm_existing_id"
    private const val KEY_EXISTING_REVISION = "alarm_existing_revision"
    private const val KEY_LABEL = "alarm_label"
    private const val KEY_HOUR = "alarm_hour"
    private const val KEY_MINUTE = "alarm_minute"
    private const val KEY_TIMEZONE = "alarm_timezone"
    private const val KEY_REPEAT = "alarm_repeat"
    private const val KEY_WEEKDAYS = "alarm_weekdays"
    private const val KEY_ENABLED = "alarm_enabled"
    private const val KEY_ONCE_DATE = "alarm_once_date"

    fun toMap(draft: AlarmEditorDraft): Map<String, Any?> = mapOf(
        KEY_EXISTING_ID to draft.existingId,
        KEY_EXISTING_REVISION to draft.existingRevision,
        KEY_LABEL to draft.label,
        KEY_HOUR to draft.hour,
        KEY_MINUTE to draft.minute,
        KEY_TIMEZONE to draft.timezone,
        KEY_REPEAT to draft.repeat.name,
        KEY_WEEKDAYS to ArrayList(draft.weekdays),
        KEY_ENABLED to draft.enabled,
        KEY_ONCE_DATE to draft.onceDate?.toString(),
    )

    /** Returns null for an empty or fundamentally malformed draft. */
    fun fromMap(values: Map<String, Any?>): AlarmEditorDraft? {
        if (values.isEmpty()) return null
        val hour = (values[KEY_HOUR] as? Number)?.toInt() ?: return null
        val minute = (values[KEY_MINUTE] as? Number)?.toInt() ?: return null
        if (hour !in 0..23 || minute !in 0..59) return null
        val timezone = values[KEY_TIMEZONE] as? String ?: return null
        if (timezone.isBlank()) return null
        val repeat = (values[KEY_REPEAT] as? String)
            ?.let { name -> RepeatKind.entries.firstOrNull { it.name == name } }
            ?: return null
        val weekdays = (values[KEY_WEEKDAYS] as? Collection<*>)
            ?.mapNotNull { (it as? Number)?.toInt() }
            ?.filter { it in 1..7 }
            ?: emptyList()
        val onceDate = (values[KEY_ONCE_DATE] as? String)
            ?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
        return AlarmEditorDraft(
            existingId = values[KEY_EXISTING_ID] as? String,
            existingRevision = (values[KEY_EXISTING_REVISION] as? Number)?.toLong(),
            label = values[KEY_LABEL] as? String ?: "",
            hour = hour,
            minute = minute,
            timezone = timezone,
            repeat = repeat,
            weekdays = weekdays,
            enabled = values[KEY_ENABLED] as? Boolean ?: true,
            onceDate = onceDate,
        )
    }
}

enum class AlarmEditorCompletion {
    SAVED,
    DELETED,
}

package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime

/** Pure draft/schedule mapping shared by the repository and its tests. */
fun AlarmDraft.toScheduleDraft(idempotencyKey: String): ScheduleDraft = ScheduleDraft(
    label = label.trim(),
    kind = ScheduleKind.ALARM,
    timezone = timezone.trim(),
    at = WallTime(hour, minute),
    onceDate = if (repeat == RepeatKind.ONCE) onceDate?.toString().orEmpty() else "",
    repeat = repeat,
    weekdays = if (repeat == RepeatKind.CUSTOM) weekdays.sorted() else emptyList(),
    enabled = enabled,
    idempotencyKey = idempotencyKey,
)

/** Applies editable fields to an existing server schedule without changing its id/revision. */
fun Schedule.withAlarmDraft(draft: AlarmDraft): Schedule = copy(
    label = draft.label.trim(),
    timezone = draft.timezone.trim(),
    at = WallTime(draft.hour, draft.minute),
    onceDate = if (draft.repeat == RepeatKind.ONCE) draft.onceDate?.toString().orEmpty() else "",
    repeat = draft.repeat,
    weekdays = if (draft.repeat == RepeatKind.CUSTOM) draft.weekdays.sorted() else emptyList(),
    enabled = draft.enabled,
)

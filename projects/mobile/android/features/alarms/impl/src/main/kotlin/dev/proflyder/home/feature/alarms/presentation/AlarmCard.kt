package dev.proflyder.home.feature.alarms.presentation

import android.content.Context
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding

/** The two shapes of an alarm row: an idle summary or a live occurrence. */
internal enum class AlarmCardMode { IDLE, ACTIVE }

/**
 * The server reports a live occurrence with `activeOccurrenceUnix`; a snoozed
 * alarm also keeps its controls until the snooze expires. Both cases are the
 * only ones where "Snooze" and "Stop" make sense, so the idle card has no
 * actions row at all.
 */
internal fun isAlarmRinging(schedule: Schedule, nowUnix: Long): Boolean =
    schedule.activeOccurrenceUnix > 0 &&
        (schedule.activeUntilUnix <= 0 || schedule.activeUntilUnix > nowUnix)

internal fun alarmCardMode(schedule: Schedule, nowUnix: Long): AlarmCardMode = when {
    schedule.isSnoozed || isAlarmRinging(schedule, nowUnix) -> AlarmCardMode.ACTIVE
    else -> AlarmCardMode.IDLE
}

/**
 * Which bulk enable actions make sense for the picked alarms: turning on is
 * only offered while at least one picked alarm is off, and turning off only
 * while at least one is on. The bar then shows one action for a uniform
 * selection and both for a mixed one.
 */
internal data class AlarmSelectionActions(
    val canEnable: Boolean,
    val canDisable: Boolean,
)

internal fun alarmSelectionActions(
    alarms: List<Schedule>,
    selectedIds: Set<String>,
): AlarmSelectionActions {
    val selected = alarms.filter { it.id in selectedIds }
    return AlarmSelectionActions(
        canEnable = selected.any { !it.enabled },
        canDisable = selected.any { it.enabled },
    )
}

/**
 * Shared by the adapter and the layout tests: text always comes from
 * resources, and the actions row is the single place that decides whether the
 * dense card grows.
 *
 * Multi-select ([actionsAllowed] false) replaces the row controls with the
 * selection indicator: the switch and delete are gone instead of disabled, so
 * the dimmed switch cannot be mistaken for the alarm's real state.
 */
internal fun bindAlarmCard(
    binding: ItemAlarmBinding,
    schedule: Schedule,
    context: Context,
    nowUnix: Long,
    actionsAllowed: Boolean,
    selected: Boolean = false,
) {
    binding.alarmTime.text = AlarmFormat.time(schedule)
    binding.alarmLabel.text = schedule.label.ifBlank { context.getString(R.string.alarms_label_default) }
    binding.alarmSummary.text = AlarmFormat.summary(context, schedule, nowUnix)
    binding.alarmEnabled.setVisible(actionsAllowed)
    binding.alarmDelete.setVisible(actionsAllowed)
    binding.alarmSelectionIndicator.setVisible(!actionsAllowed)
    binding.alarmSelectionCheck.setImageResource(
        if (selected) R.drawable.alarms_ic_selection_on else R.drawable.alarms_ic_selection_off,
    )
    binding.alarmActions.setVisible(
        actionsAllowed && alarmCardMode(schedule, nowUnix) == AlarmCardMode.ACTIVE,
    )
}

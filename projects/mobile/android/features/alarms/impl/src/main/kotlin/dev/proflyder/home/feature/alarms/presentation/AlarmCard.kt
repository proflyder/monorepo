package dev.proflyder.home.feature.alarms.presentation

import android.content.Context
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding

internal enum class AlarmCardMode { IDLE, ACTIVE }

internal fun isAlarmRinging(schedule: Schedule, nowUnix: Long): Boolean =
    schedule.activeOccurrenceUnix > 0 &&
        (schedule.activeUntilUnix <= 0 || schedule.activeUntilUnix > nowUnix)

internal fun alarmCardMode(schedule: Schedule, nowUnix: Long): AlarmCardMode = when {
    schedule.isSnoozed || isAlarmRinging(schedule, nowUnix) -> AlarmCardMode.ACTIVE
    else -> AlarmCardMode.IDLE
}

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

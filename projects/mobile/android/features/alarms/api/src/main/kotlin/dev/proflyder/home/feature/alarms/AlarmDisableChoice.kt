package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule

enum class AlarmDisableChoice {
    DISABLE,

    ASK,
}

fun alarmDisableChoice(schedule: Schedule): AlarmDisableChoice = when (schedule.repeat) {
    RepeatKind.DAILY, RepeatKind.WEEKDAYS, RepeatKind.WEEKENDS, RepeatKind.CUSTOM -> AlarmDisableChoice.ASK
    RepeatKind.ONCE, RepeatKind.UNSPECIFIED -> AlarmDisableChoice.DISABLE
}

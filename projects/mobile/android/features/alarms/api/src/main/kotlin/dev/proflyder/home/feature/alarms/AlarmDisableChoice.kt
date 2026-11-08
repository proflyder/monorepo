package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule

/** What turning an alarm off means for its recurrence. */
enum class AlarmDisableChoice {
    /** One-shot alarms have nothing to repeat: a plain disable is enough. */
    DISABLE,

    /** Recurring alarms ask whether to skip one occurrence or disable entirely. */
    ASK,
}

/**
 * Recurring alarms cannot be "off" without losing the rule, so the UI asks
 * first. A one-shot alarm keeps the previous behaviour: the switch just turns
 * it off.
 */
fun alarmDisableChoice(schedule: Schedule): AlarmDisableChoice = when (schedule.repeat) {
    RepeatKind.DAILY, RepeatKind.WEEKDAYS, RepeatKind.WEEKENDS, RepeatKind.CUSTOM -> AlarmDisableChoice.ASK
    RepeatKind.ONCE, RepeatKind.UNSPECIFIED -> AlarmDisableChoice.DISABLE
}

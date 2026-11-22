package dev.proflyder.home.core.testing

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.model.WallTime

object ScheduleFixtures {
    fun schedule(
        id: String,
        hour: Int = 7,
        minute: Int = 0,
        label: String = id,
        kind: ScheduleKind = ScheduleKind.ALARM,
        repeat: RepeatKind = RepeatKind.DAILY,
        enabled: Boolean = true,
        onceDate: String = "",
    ) = Schedule(
        id = id,
        label = label,
        kind = kind,
        at = WallTime(hour, minute),
        onceDate = onceDate,
        repeat = repeat,
        weekdays = emptyList(),
        nextDueUnix = 0,
        enabled = enabled,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
    )

    fun timer(
        id: String,
        dueUnix: Long,
        state: TimerState = TimerState.RUNNING,
        durationSeconds: Long = 60,
        createdUnix: Long = 0,
    ) = Timer(
        id = id,
        label = id,
        durationSeconds = durationSeconds,
        remainingSeconds = durationSeconds,
        dueUnix = dueUnix,
        state = state,
        revision = 1,
        createdUnix = createdUnix,
        updatedUnix = 0,
    )
}

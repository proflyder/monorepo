package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState

internal sealed interface HubCard {
    val key: String

    data class TimerCard(val timer: Timer) : HubCard {
        override val key: String get() = "timer:${timer.id}"
    }

    data class Reminder(val schedule: Schedule) : HubCard {
        override val key: String get() = "reminder:${schedule.id}"
    }
}

internal object HubFormat {
    fun timerRemaining(timer: Timer, nowUnix: Long): Long = when (timer.state) {
        TimerState.RUNNING -> (timer.dueUnix - nowUnix).coerceAtLeast(0L)
        TimerState.PAUSED -> timer.remainingSeconds.coerceAtLeast(0L)
        else -> 0L
    }
}

private data class HubRow(val card: HubCard, val atUnix: Long)

internal object HubCards {
    const val MAX_ROWS = 3

    const val REMINDER_WINDOW_SECONDS = 24L * 60L * 60L

    fun build(
        timers: List<Timer>,
        reminders: List<Schedule>,
        nowUnix: Long,
    ): List<HubCard> {
        val rows = mutableListOf<HubRow>()
        timers.forEach { addTimer(it, nowUnix, rows) }
        reminders.forEach { addReminder(it, nowUnix, rows) }
        return rows
            .sortedWith(compareBy({ it.atUnix }, { it.card.key }))
            .take(MAX_ROWS)
            .map { it.card }
    }

    private fun addTimer(timer: Timer, nowUnix: Long, rows: MutableList<HubRow>) {
        val remaining = HubFormat.timerRemaining(timer, nowUnix)
        if (!timer.isActive || remaining <= 0) return
        val at = if (timer.state == TimerState.PAUSED) nowUnix + remaining else timer.dueUnix
        rows += HubRow(HubCard.TimerCard(timer), at)
    }

    private fun addReminder(reminder: Schedule, nowUnix: Long, rows: MutableList<HubRow>) {
        if (!reminder.enabled || reminder.nextDueUnix <= 0) return
        val lead = reminder.nextDueUnix - nowUnix
        if (lead !in 1L..REMINDER_WINDOW_SECONDS) return
        rows += HubRow(HubCard.Reminder(reminder), reminder.nextDueUnix)
    }
}

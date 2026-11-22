package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

internal sealed interface HubCard {
    val key: String

    data class Alarm(val schedule: Schedule, val ringing: Boolean) : HubCard {
        override val key: String get() = "alarm:${schedule.id}"
    }

    data class TimerCard(val timer: Timer) : HubCard {
        override val key: String get() = "timer:${timer.id}"
    }

    data class Reminder(val schedule: Schedule) : HubCard {
        override val key: String get() = "reminder:${schedule.id}"
    }
}

internal interface HubScheduleStrings {
    fun today(): String
    fun tomorrow(): String

    fun dayOfWeek(isoDay: Int): String

    fun time(hour: Int, minute: Int): String
}

internal object HubFormat {
    private fun farDate(): DateTimeFormatter =
        DateTimeFormatter.ofPattern("d MMM", Locale.getDefault())

    fun whenText(
        epochSeconds: Long,
        zone: ZoneId,
        nowUnix: Long,
        strings: HubScheduleStrings,
    ): String {
        val due = Instant.ofEpochSecond(epochSeconds).atZone(zone).toLocalDateTime()
        val today = Instant.ofEpochSecond(nowUnix).atZone(zone).toLocalDate()
        val time = strings.time(due.hour, due.minute)
        val date = due.toLocalDate()
        return when {
            date == today -> "${strings.today()} $time"
            date == today.plusDays(1) -> "${strings.tomorrow()} $time"
            date.isBefore(today.plusDays(7)) -> "${strings.dayOfWeek(due.dayOfWeek.value)} $time"
            else -> "${date.format(farDate())} $time"
        }
    }

    fun isRinging(schedule: Schedule, nowUnix: Long): Boolean =
        schedule.activeOccurrenceUnix > 0 &&
            (schedule.activeUntilUnix <= 0 || schedule.activeUntilUnix > nowUnix)

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
        alarms: List<Schedule>,
        timers: List<Timer>,
        reminders: List<Schedule>,
        nowUnix: Long,
    ): List<HubCard> {
        val rows = mutableListOf<HubRow>()
        alarms.forEach { addAlarm(it, nowUnix, rows) }
        timers.forEach { addTimer(it, nowUnix, rows) }
        reminders.forEach { addReminder(it, nowUnix, rows) }
        return rows
            .sortedWith(compareBy({ it.atUnix }, { it.card.key }))
            .take(MAX_ROWS)
            .map { it.card }
    }

    private fun addAlarm(alarm: Schedule, nowUnix: Long, rows: MutableList<HubRow>) {
        val ringing = HubFormat.isRinging(alarm, nowUnix)
        if (!ringing && (!alarm.enabled || alarm.nextDueUnix <= 0)) return
        val at = if (ringing) nowUnix else alarm.nextDueUnix
        rows += HubRow(HubCard.Alarm(alarm, ringing), at)
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

    fun today(zone: ZoneId, nowUnix: Long): LocalDate =
        Instant.ofEpochSecond(nowUnix).atZone(zone).toLocalDate()

    fun buildOrKeep(
        alarms: List<Schedule>?,
        timers: List<Timer>?,
        reminders: List<Schedule>?,
        previous: List<HubCard>,
        nowUnix: Long,
    ): List<HubCard> = build(
        alarms = alarms ?: previous.filterIsInstance<HubCard.Alarm>().map { it.schedule },
        timers = timers ?: previous.filterIsInstance<HubCard.TimerCard>().map { it.timer },
        reminders = reminders ?: previous.filterIsInstance<HubCard.Reminder>().map { it.schedule },
        nowUnix = nowUnix,
    )
}

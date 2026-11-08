package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * One live card of the Home carousel. Only cards that are relevant right now
 * are built; an empty list means the carousel is hidden.
 */
sealed interface HubCard {
    /** Stable identity for the pager adapter. */
    val key: String

    data class Alarm(val schedule: Schedule, val ringing: Boolean) : HubCard {
        override val key: String get() = "alarm:${schedule.id}"
    }

    data class TimerCard(val timer: Timer) : HubCard {
        override val key: String get() = "timer:${timer.id}"
    }

    data class ScenarioCard(val scenario: Scenario) : HubCard {
        override val key: String get() = "scenario:${scenario.id}"
    }

    data class Reminder(val schedule: Schedule) : HubCard {
        override val key: String get() = "reminder:${schedule.id}"
    }
}

/** Localized date words for the carousel; the implementation supplies resources. */
interface HubScheduleStrings {
    fun today(): String
    fun tomorrow(): String

    /** Short weekday name for ISO day 1..7 (Monday = 1). */
    fun dayOfWeek(isoDay: Int): String

    fun time(hour: Int, minute: Int): String
}

object HubFormat {
    /** Far dates are rendered with the current locale; built per call so a locale change is honoured. */
    private fun farDate(): DateTimeFormatter =
        DateTimeFormatter.ofPattern("d MMM", Locale.getDefault())

    /**
     * "сегодня 07:30", "завтра 07:30", "пн 07:30" or "12 нояб. 07:30".
     * [nowUnix] is the scheduler time, so the phone wall clock cannot shift it.
     */
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

    fun zoneOf(timezone: String): ZoneId =
        runCatching { ZoneId.of(timezone) }.getOrDefault(ZoneId.systemDefault())

    /** True while the scheduler reports a live occurrence for this alarm. */
    fun isRinging(schedule: Schedule, nowUnix: Long): Boolean =
        schedule.activeOccurrenceUnix > 0 &&
            (schedule.activeUntilUnix <= 0 || schedule.activeUntilUnix > nowUnix)

    /** Remaining seconds of a timer card, using the shared server clock. */
    fun timerRemaining(timer: Timer, nowUnix: Long): Long = when (timer.state) {
        TimerState.RUNNING -> (timer.dueUnix - nowUnix).coerceAtLeast(0L)
        TimerState.PAUSED -> timer.remainingSeconds.coerceAtLeast(0L)
        else -> 0L
    }
}

/**
 * Selects the live cards: the nearest enabled alarm (or the ringing one), every
 * running/paused timer, the next native scenario and the next reminder.
 */
object HubCards {
    fun build(
        alarms: List<Schedule>,
        timers: List<Timer>,
        scenarios: List<Scenario>,
        reminders: List<Schedule>,
        nowUnix: Long,
    ): List<HubCard> {
        val cards = mutableListOf<HubCard>()

        val ringing = alarms
            .filter { HubFormat.isRinging(it, nowUnix) }
            .minByOrNull { it.activeOccurrenceUnix }
        val nearestAlarm = alarms
            .filter { it.enabled && it.nextDueUnix > 0 && it.id != ringing?.id }
            .minByOrNull { it.nextDueUnix }
        (ringing ?: nearestAlarm)?.let { alarm ->
            cards += HubCard.Alarm(alarm, ringing = alarm.id == ringing?.id)
        }

        timers
            .filter { it.isActive && HubFormat.timerRemaining(it, nowUnix) > 0 }
            .sortedWith(
                compareBy(
                    { if (it.state == TimerState.RUNNING) 0 else 1 },
                    { it.dueUnix },
                    { it.createdUnix },
                ),
            )
            .forEach { cards += HubCard.TimerCard(it) }

        scenarios
            .filter { it.enabled && it.nextDueUnix > 0 }
            .minWithOrNull(compareBy({ it.nextDueUnix }, { it.label.lowercase() }))
            ?.let { cards += HubCard.ScenarioCard(it) }

        reminders
            .filter { it.enabled && it.nextDueUnix > 0 }
            .minWithOrNull(compareBy({ it.nextDueUnix }, { it.label.lowercase() }))
            ?.let { cards += HubCard.Reminder(it) }

        return cards
    }

    /** Today's date in the scheduler time zone; used by callers that need a date for text. */
    fun today(zone: ZoneId, nowUnix: Long): LocalDate =
        Instant.ofEpochSecond(nowUnix).atZone(zone).toLocalDate()
}

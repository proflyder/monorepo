package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZonedDateTime

data class AlarmCountdown(val days: Long, val hours: Long, val minutes: Long) {
    val underMinute: Boolean get() = days == 0L && hours == 0L && minutes == 0L
}

val ALARM_COUNTDOWN_CARD_WINDOW: Duration = Duration.ofHours(24)

private const val WEEK_DAYS = 7L

fun alarmCountdown(now: Instant, trigger: Instant): AlarmCountdown {
    val total = Duration.between(now, trigger).coerceAtLeast(Duration.ZERO).toMinutes()
    return AlarmCountdown(days = total / MINUTES_PER_DAY, hours = total % MINUTES_PER_DAY / MINUTES_PER_HOUR, minutes = total % MINUTES_PER_HOUR)
}

private const val MINUTES_PER_HOUR = 60L
private const val MINUTES_PER_DAY = 24L * MINUTES_PER_HOUR

fun repeatDays(repeat: RepeatKind, weekdays: Collection<Int>): Set<Int> = when (repeat) {
    RepeatKind.DAILY -> (1..WEEK_DAYS.toInt()).toSet()
    RepeatKind.WEEKDAYS -> (1..5).toSet()
    RepeatKind.WEEKENDS -> setOf(6, 7)
    RepeatKind.CUSTOM -> weekdays.toSet()
    else -> emptySet()
}

fun AlarmDraft.nextTrigger(now: Instant, zone: ZoneId): Instant? {
    if (!enabled) return null
    val time = LocalTime.of(hour, minute)
    return when (repeat) {
        RepeatKind.ONCE -> onceTrigger(onceDate, time, now, zone)
        else -> repeatingTrigger(repeatDays(repeat, weekdays), time, now, zone)
    }
}

fun Schedule.nextTrigger(now: Instant, zone: ZoneId): Instant? {
    val server = Instant.ofEpochSecond(nextDueUnix).takeIf { enabled && nextDueUnix > 0 && it.isAfter(now) }
    return server ?: toAlarmDraft().nextTrigger(now, zone)
}

fun Instant.withinCountdownWindow(now: Instant): Boolean =
    isAfter(now) && Duration.between(now, this) <= ALARM_COUNTDOWN_CARD_WINDOW

private fun onceTrigger(date: LocalDate?, time: LocalTime, now: Instant, zone: ZoneId): Instant? {
    val today = now.atZone(zone).toLocalDate()
    val candidates = date?.let { listOf(it) } ?: listOf(today, today.plusDays(1))
    return candidates.map { at(it, time, zone) }.firstOrNull { it.isAfter(now) }
}

private fun repeatingTrigger(days: Set<Int>, time: LocalTime, now: Instant, zone: ZoneId): Instant? {
    val today = now.atZone(zone).toLocalDate()
    return (0..WEEK_DAYS).asSequence()
        .map { today.plusDays(it) }
        .filter { it.dayOfWeek.value in days }
        .map { at(it, time, zone) }
        .firstOrNull { it.isAfter(now) }
}

private fun at(date: LocalDate, time: LocalTime, zone: ZoneId): Instant = ZonedDateTime.of(date, time, zone).toInstant()

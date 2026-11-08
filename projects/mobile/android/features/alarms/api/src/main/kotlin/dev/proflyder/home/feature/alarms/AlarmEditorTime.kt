package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.WallTime
import java.time.LocalDateTime
import java.time.temporal.ChronoUnit

/**
 * Quick "now + N" offsets shown under the editor's time picker, in minutes and
 * display order. The editor keeps this list as the single source of truth.
 */
val ALARM_QUICK_OFFSETS_MINUTES: List<Int> = listOf(15, 30, 60, 90, 120)

/**
 * Wall time pre-filled for a brand-new alarm: the next full hour strictly
 * after [now]. Rounding up rather than to the nearest hour keeps the default
 * in the future even exactly on the hour (14:00 → 15:00, 14:20 → 15:00).
 */
fun defaultAlarmTime(now: LocalDateTime): WallTime {
    val next = now.truncatedTo(ChronoUnit.HOURS).plusHours(1)
    return WallTime(next.hour, next.minute)
}

/**
 * Wall time [offsetMinutes] after [now]. The offset is always counted from the
 * current time, never from a previously selected value, so repeated taps do
 * not accumulate. Minutes are not rounded and crossing midnight wraps.
 */
fun quickOffsetTime(now: LocalDateTime, offsetMinutes: Int): WallTime {
    require(offsetMinutes >= 0) { "quick offset must be non-negative" }
    val target = now.plusMinutes(offsetMinutes.toLong())
    return WallTime(target.hour, target.minute)
}

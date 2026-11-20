package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.WallTime
import java.time.LocalDateTime
import java.time.temporal.ChronoUnit

val ALARM_QUICK_OFFSETS_MINUTES: List<Int> = listOf(15, 30, 60, 90, 120)

fun defaultAlarmTime(now: LocalDateTime): WallTime {
    val next = now.truncatedTo(ChronoUnit.HOURS).plusHours(1)
    return WallTime(next.hour, next.minute)
}

fun quickOffsetTime(now: LocalDateTime, offsetMinutes: Int): WallTime {
    require(offsetMinutes >= 0) { "quick offset must be non-negative" }
    val target = now.plusMinutes(offsetMinutes.toLong())
    return WallTime(target.hour, target.minute)
}

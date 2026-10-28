package dev.proflyder.home.feature.alarms.presentation

import android.content.Context
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.alarms.impl.R
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

internal object AlarmFormat {
    private val dayFormatter = DateTimeFormatter.ofPattern("EEE d MMM", Locale.getDefault())

    fun time(schedule: Schedule): String =
        String.format(Locale.ROOT, "%02d:%02d", schedule.at.hour, schedule.at.minute)

    fun repeatSummary(context: Context, schedule: Schedule): String = when (schedule.repeat) {
        RepeatKind.ONCE -> {
            val date = runCatching { LocalDate.parse(schedule.onceDate) }.getOrNull()
            if (date == null) {
                context.getString(R.string.alarms_repeat_once)
            } else {
                context.getString(R.string.alarms_repeat_once_on, date.format(dayFormatter))
            }
        }
        RepeatKind.DAILY -> context.getString(R.string.alarms_repeat_daily)
        RepeatKind.WEEKDAYS -> context.getString(R.string.alarms_repeat_weekdays)
        RepeatKind.WEEKENDS -> context.getString(R.string.alarms_repeat_weekends)
        RepeatKind.CUSTOM -> customDays(context, schedule.weekdays)
        else -> context.getString(R.string.alarms_repeat_unknown)
    }

    fun customDays(context: Context, days: List<Int>): String {
        if (days.isEmpty()) return context.getString(R.string.alarms_repeat_unknown)
        val labels = context.resources.getStringArray(R.array.alarms_day_short)
        return days.sorted().joinToString(", ") { day ->
            labels.getOrElse(day - 1) { day.toString() }
        }
    }

    fun nextDue(context: Context, schedule: Schedule): String {
        if (!schedule.enabled) return context.getString(R.string.alarms_off)
        if (schedule.snoozeUntilUnix > 0) {
            return context.getString(R.string.alarms_snoozed, instant(schedule.snoozeUntilUnix, schedule.timezone))
        }
        if (schedule.nextDueUnix <= 0) return context.getString(R.string.alarms_off)
        val zone = runCatching { ZoneId.of(schedule.timezone) }.getOrDefault(ZoneId.systemDefault())
        val due = Instant.ofEpochSecond(schedule.nextDueUnix).atZone(zone).toLocalDateTime()
        val today = LocalDate.now(zone)
        val datePart = when (due.toLocalDate()) {
            today -> context.getString(R.string.alarms_today, due.format(timeOnly))
            today.plusDays(1) -> context.getString(R.string.alarms_tomorrow, due.format(timeOnly))
            else -> due.format(dayFormatter) + " " + due.format(timeOnly)
        }
        return context.getString(R.string.alarms_next, datePart)
    }

    private fun instant(epochSeconds: Long, timezone: String): String {
        val zone = runCatching { ZoneId.of(timezone) }.getOrDefault(ZoneId.systemDefault())
        return LocalDateTime.ofInstant(Instant.ofEpochSecond(epochSeconds), zone).format(dayTime)
    }

    private val timeOnly: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm", Locale.ROOT)
    private val dayTime: DateTimeFormatter = DateTimeFormatter.ofPattern("d MMM HH:mm", Locale.getDefault())
}

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
    private fun dayFormatter() = DateTimeFormatter.ofPattern("EEE d MMM", Locale.getDefault())

    fun time(schedule: Schedule): String =
        String.format(Locale.ROOT, "%02d:%02d", schedule.at.hour, schedule.at.minute)

    fun repeatSummary(context: Context, schedule: Schedule): String = when (schedule.repeat) {
        RepeatKind.ONCE -> {
            val date = runCatching { LocalDate.parse(schedule.onceDate) }.getOrNull()
            if (date == null) {
                context.getString(R.string.alarms_repeat_once_summary)
            } else {
                context.getString(R.string.alarms_repeat_once_on, date.format(dayFormatter()))
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
        return dayList(context, days)
    }

    fun repeatValue(context: Context, repeat: RepeatKind, weekdays: Set<Int>): String = when (repeat) {
        RepeatKind.ONCE -> context.getString(R.string.alarms_repeat_once)
        RepeatKind.DAILY -> context.getString(R.string.alarms_repeat_daily)
        RepeatKind.WEEKDAYS -> context.getString(R.string.alarms_repeat_weekdays)
        RepeatKind.WEEKENDS -> context.getString(R.string.alarms_repeat_weekends)
        RepeatKind.CUSTOM -> when (weekdays.isEmpty()) {
            true -> context.getString(R.string.alarms_repeat_custom)
            false -> dayList(context, weekdays.toList())
        }
        else -> context.getString(R.string.alarms_repeat_unknown)
    }

    private fun dayList(context: Context, days: List<Int>): String {
        val labels = context.resources.getStringArray(R.array.alarms_day_short)
        return days.sorted().joinToString(", ") { day ->
            labels.getOrElse(day - 1) { day.toString() }
        }
    }

    fun summary(context: Context, schedule: Schedule, nowUnix: Long): String {
        val repeat = repeatSummary(context, schedule)
        return when {
            isAlarmRinging(schedule, nowUnix) -> context.getString(R.string.alarms_card_ringing, repeat)
            schedule.isSnoozed -> context.getString(
                R.string.alarms_card_snoozed,
                repeat,
                instant(schedule.snoozeUntilUnix),
            )
            !schedule.enabled || schedule.nextDueUnix <= 0 -> context.getString(R.string.alarms_card_off, repeat)
            else -> upcoming(context, schedule, repeat, Instant.ofEpochSecond(nowUnix))
        }
    }

    private fun upcoming(context: Context, schedule: Schedule, repeat: String, now: Instant): String =
        AlarmCountdownText.card(context.resources, schedule, now, ZoneId.systemDefault())
            ?.let { context.getString(R.string.alarms_card_countdown, repeat, it) }
            ?: context.getString(R.string.alarms_card_next, repeat, nextDay(context, schedule))

    private fun nextDay(context: Context, schedule: Schedule): String {
        val zone = ZoneId.systemDefault()
        val due = Instant.ofEpochSecond(schedule.nextDueUnix).atZone(zone).toLocalDateTime()
        val today = LocalDate.now(zone)
        return when (due.toLocalDate()) {
            today -> context.getString(R.string.alarms_today, due.format(timeOnly))
            today.plusDays(1) -> context.getString(R.string.alarms_tomorrow, due.format(timeOnly))
            else -> due.format(dayFormatter()) + " " + due.format(timeOnly)
        }
    }

    private fun instant(epochSeconds: Long): String =
        LocalDateTime.ofInstant(Instant.ofEpochSecond(epochSeconds), ZoneId.systemDefault()).format(dayTime())

    private val timeOnly: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm", Locale.ROOT)
    private fun dayTime() = DateTimeFormatter.ofPattern("d MMM HH:mm", Locale.getDefault())
}

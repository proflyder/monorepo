package dev.proflyder.home.feature.reminders.presentation

import android.content.Context
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.reminders.impl.R
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.Locale

internal object ReminderFormat {
    private val dateFormat: DateTimeFormatter = DateTimeFormatter.ofPattern("EEE d MMM", Locale.getDefault())

    fun due(context: Context, schedule: Schedule): String {
        if (!schedule.enabled) return context.getString(R.string.reminders_off)
        val date = runCatching { LocalDate.parse(schedule.onceDate) }.getOrNull()
            ?: return context.getString(R.string.reminders_off)
        val text = String.format(
            Locale.ROOT,
            "%s %02d:%02d",
            date.format(dateFormat),
            schedule.at.hour,
            schedule.at.minute,
        )
        return context.getString(R.string.reminders_due, text)
    }

    fun dateLabel(date: LocalDate): String = date.format(dateFormat)

    fun timeLabel(hour: Int, minute: Int): String =
        String.format(Locale.ROOT, "%02d:%02d", hour, minute)
}

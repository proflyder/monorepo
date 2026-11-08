package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.HubFormat
import dev.proflyder.home.feature.control.HubScheduleStrings
import dev.proflyder.home.feature.control.impl.R

/** Resource-backed text for the compact live rows. */
internal class HubCardTexts(private val context: Context) : HubScheduleStrings {
    override fun today(): String = context.getString(R.string.control_hub_today)
    override fun tomorrow(): String = context.getString(R.string.control_hub_tomorrow)
    override fun dayOfWeek(isoDay: Int): String =
        context.resources.getStringArray(R.array.control_days_short).getOrElse(isoDay - 1) { isoDay.toString() }

    override fun time(hour: Int, minute: Int): String =
        String.format(java.util.Locale.ROOT, "%02d:%02d", hour, minute)

    /** The row's own name, used by the label column and as the toggle description. */
    fun title(card: HubCard): String = when (card) {
        is HubCard.Alarm -> card.schedule.label.ifBlank { context.getString(R.string.control_hub_alarm) }
        is HubCard.TimerCard -> card.timer.label.ifBlank { context.getString(R.string.control_hub_timer) }
        is HubCard.Reminder -> card.schedule.label.ifBlank { context.getString(R.string.control_hub_reminder) }
    }

    /** The label column: an alarm also carries its repeat days (or "ringing"). */
    fun label(card: HubCard): String = when (card) {
        is HubCard.Alarm -> alarmLabel(card)
        is HubCard.TimerCard, is HubCard.Reminder -> title(card)
    }

    private fun alarmLabel(card: HubCard.Alarm): String {
        val suffix = if (card.ringing) {
            context.getString(R.string.control_hub_ringing)
        } else {
            alarmRepeat(card.schedule)
        }
        return context.getString(R.string.control_hub_alarm_subtitle, title(card), suffix)
    }

    private fun alarmRepeat(schedule: Schedule): String =
        when (schedule.repeat) {
            RepeatKind.DAILY -> context.getString(R.string.control_hub_repeat_daily)
            RepeatKind.WEEKDAYS -> context.getString(R.string.control_hub_repeat_weekdays)
            RepeatKind.WEEKENDS -> context.getString(R.string.control_hub_repeat_weekends)
            RepeatKind.CUSTOM -> customDays(schedule.weekdays)
            RepeatKind.ONCE -> context.getString(R.string.control_hub_repeat_once)
            RepeatKind.UNSPECIFIED -> context.getString(R.string.control_hub_repeat_unknown)
        }

    private fun customDays(days: List<Int>): String {
        if (days.isEmpty()) return context.getString(R.string.control_hub_repeat_unknown)
        return days.sorted().joinToString(", ") { dayOfWeek(it) }
    }

    /** The time column: a clock moment, or a live countdown for a timer. */
    fun timeText(card: HubCard, remaining: Long?, nowUnix: Long): String = when (card) {
        is HubCard.Alarm -> alarmTimeText(card, nowUnix)
        is HubCard.TimerCard -> DurationText.format(
            remaining ?: HubFormat.timerRemaining(card.timer, nowUnix),
        )
        is HubCard.Reminder -> HubFormat.whenText(
            card.schedule.nextDueUnix,
            HubFormat.zoneOf(card.schedule.timezone),
            nowUnix,
            this,
        )
    }

    private fun alarmTimeText(card: HubCard.Alarm, nowUnix: Long): String {
        val due = if (card.ringing) card.schedule.activeOccurrenceUnix else card.schedule.nextDueUnix
        return HubFormat.whenText(
            due,
            HubFormat.zoneOf(card.schedule.timezone),
            nowUnix,
            this,
        )
    }
}

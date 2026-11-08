package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.HubFormat
import dev.proflyder.home.feature.control.HubScheduleStrings
import dev.proflyder.home.feature.control.impl.R
import java.time.ZoneId

/** Resource-backed text for the live Home carousel. */
internal class HubCardTexts(private val context: Context) : HubScheduleStrings {
    override fun today(): String = context.getString(R.string.control_hub_today)
    override fun tomorrow(): String = context.getString(R.string.control_hub_tomorrow)
    override fun dayOfWeek(isoDay: Int): String =
        context.resources.getStringArray(R.array.control_days_short).getOrElse(isoDay - 1) { isoDay.toString() }

    override fun time(hour: Int, minute: Int): String =
        String.format(java.util.Locale.ROOT, "%02d:%02d", hour, minute)

    fun title(card: HubCard): String = when (card) {
        is HubCard.Alarm -> card.schedule.label.ifBlank { context.getString(R.string.control_hub_alarm) }
        is HubCard.TimerCard -> card.timer.label.ifBlank { context.getString(R.string.control_hub_timer) }
        is HubCard.ScenarioCard -> card.scenario.label.ifBlank { context.getString(R.string.control_hub_scenario) }
        is HubCard.Reminder -> card.schedule.label.ifBlank { context.getString(R.string.control_hub_reminder) }
    }

    fun summary(card: HubCard, remaining: Long?, nowUnix: Long): String = when (card) {
        is HubCard.Alarm -> if (card.ringing) {
            context.getString(R.string.control_hub_ringing)
        } else {
            HubFormat.whenText(
                card.schedule.nextDueUnix,
                HubFormat.zoneOf(card.schedule.timezone),
                nowUnix,
                this,
            )
        }
        is HubCard.TimerCard -> context.getString(
            R.string.control_hub_timer_left,
            DurationText.format(remaining ?: HubFormat.timerRemaining(card.timer, nowUnix)),
        )
        is HubCard.ScenarioCard -> HubFormat.whenText(
            card.scenario.nextDueUnix,
            ZoneId.systemDefault(),
            nowUnix,
            this,
        )
        is HubCard.Reminder -> HubFormat.whenText(
            card.schedule.nextDueUnix,
            HubFormat.zoneOf(card.schedule.timezone),
            nowUnix,
            this,
        )
    }

    fun primaryLabel(card: HubCard): String? = when (card) {
        is HubCard.Alarm -> when {
            card.ringing -> context.getString(R.string.control_hub_stop)
            card.schedule.enabled -> context.getString(R.string.control_hub_turn_off)
            else -> context.getString(R.string.control_hub_turn_on)
        }
        is HubCard.TimerCard -> context.getString(
            if (card.timer.state == TimerState.PAUSED) R.string.control_hub_resume else R.string.control_hub_pause,
        )
        is HubCard.ScenarioCard -> context.getString(R.string.control_hub_run)
        is HubCard.Reminder -> null
    }

    fun secondaryLabel(card: HubCard): String? = when (card) {
        is HubCard.Alarm -> if (card.ringing) context.getString(R.string.control_hub_snooze) else null
        is HubCard.TimerCard -> context.getString(R.string.control_hub_stop)
        is HubCard.ScenarioCard, is HubCard.Reminder -> null
    }
}

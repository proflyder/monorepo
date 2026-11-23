package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.time.DueText
import dev.proflyder.home.core.time.DueTextStrings
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.HubFormat
import dev.proflyder.home.feature.control.impl.R
import java.time.ZoneId

internal class HubCardTexts(private val context: Context) : DueTextStrings {
    override fun today(): String = context.getString(R.string.control_hub_today)
    override fun tomorrow(): String = context.getString(R.string.control_hub_tomorrow)
    override fun dayOfWeek(isoDay: Int): String =
        context.resources.getStringArray(R.array.control_days_short).getOrElse(isoDay - 1) { isoDay.toString() }

    fun label(card: HubCard): String = when (card) {
        is HubCard.TimerCard -> card.timer.label.ifBlank { context.getString(R.string.control_hub_timer) }
        is HubCard.Reminder -> card.schedule.label.ifBlank { context.getString(R.string.control_hub_reminder) }
    }

    fun timeText(card: HubCard, remaining: Long?, nowUnix: Long): String = when (card) {
        is HubCard.TimerCard -> DurationText.format(remaining ?: HubFormat.timerRemaining(card.timer, nowUnix))
        is HubCard.Reminder -> DueText.format(card.schedule.nextDueUnix, ZoneId.systemDefault(), nowUnix, this)
    }
}

package dev.proflyder.home.feature.alarms.presentation

import android.content.res.Resources
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.alarms.AlarmCountdown
import dev.proflyder.home.feature.alarms.alarmCountdown
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.nextTrigger
import dev.proflyder.home.feature.alarms.withinCountdownWindow
import java.time.Instant
import java.time.ZoneId

internal object AlarmCountdownText {
    fun relative(resources: Resources, countdown: AlarmCountdown): String = when (countdown.underMinute) {
        true -> resources.getString(R.string.alarms_countdown_under_minute)
        false -> resources.getString(R.string.alarms_countdown_in, parts(resources, countdown))
    }

    fun notice(resources: Resources, now: Instant, trigger: Instant): String =
        resources.getString(R.string.alarms_countdown_notice, relative(resources, alarmCountdown(now, trigger)))

    fun card(resources: Resources, schedule: Schedule, now: Instant, zone: ZoneId): String? =
        schedule.nextTrigger(now, zone)
            ?.takeIf { it.withinCountdownWindow(now) }
            ?.let { relative(resources, alarmCountdown(now, it)) }

    private fun parts(resources: Resources, countdown: AlarmCountdown): String = listOf(
        R.plurals.alarms_countdown_days to countdown.days,
        R.plurals.alarms_countdown_hours to countdown.hours,
        R.plurals.alarms_countdown_minutes to countdown.minutes,
    )
        .filter { (_, value) -> value > 0 }
        .joinToString(" ") { (id, value) -> resources.getQuantityString(id, value.toInt(), value.toInt()) }
}

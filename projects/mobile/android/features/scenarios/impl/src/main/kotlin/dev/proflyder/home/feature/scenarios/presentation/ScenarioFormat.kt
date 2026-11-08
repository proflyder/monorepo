package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.feature.scenarios.ScenarioTriggerStrings
import dev.proflyder.home.feature.scenarios.describeTriggers
import dev.proflyder.home.feature.scenarios.impl.R
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** Resource-backed presentation text for scenario rows and editor cards. */
internal class ResourceTriggerStrings(private val context: Context) : ScenarioTriggerStrings {
    override fun manual(): String = context.getString(R.string.scenarios_trigger_manual)
    override fun daily(): String = context.getString(R.string.scenarios_trigger_daily)
    override fun weekdays(): String = context.getString(R.string.scenarios_trigger_weekdays)
    override fun weekends(): String = context.getString(R.string.scenarios_trigger_weekends)
    override fun once(date: LocalDate?): String =
        if (date == null) {
            context.getString(R.string.scenarios_trigger_once)
        } else {
            context.getString(R.string.scenarios_trigger_once_on, date.format(ScenarioFormat.shortDate()))
        }
    override fun custom(days: List<Int>): String =
        if (days.isEmpty()) {
            context.getString(R.string.scenarios_trigger_custom)
        } else {
            context.getString(R.string.scenarios_trigger_custom_days, ScenarioFormat.dayNames(context, days))
        }
    override fun unsupported(): String = context.getString(R.string.scenarios_trigger_unsupported)
    override fun time(hour: Int, minute: Int): String =
        String.format(Locale.ROOT, "%02d:%02d", hour, minute)
}

internal object ScenarioFormat {
    private val timeOnly: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm", Locale.ROOT)

    fun triggerSummary(context: Context, scenario: Scenario): String =
        describeTriggers(scenario.triggers, ResourceTriggerStrings(context))

    fun nextDue(context: Context, scenario: Scenario): String {
        if (!scenario.enabled) return context.getString(R.string.scenarios_disabled)
        if (scenario.nextDueUnix <= 0) return context.getString(R.string.scenarios_no_next_run)
        val zone = ZoneId.systemDefault()
        val due = Instant.ofEpochSecond(scenario.nextDueUnix).atZone(zone).toLocalDateTime()
        val today = LocalDate.now(zone)
        val datePart = when (due.toLocalDate()) {
            today -> context.getString(R.string.scenarios_today, due.format(timeOnly))
            today.plusDays(1) -> context.getString(R.string.scenarios_tomorrow, due.format(timeOnly))
            else -> due.format(shortDate()) + " " + due.format(timeOnly)
        }
        return context.getString(R.string.scenarios_next, datePart)
    }

    fun describeCondition(
        context: Context,
        condition: ScenarioCondition,
        entryTitle: (String) -> String?,
    ): String = when (condition) {
        is ScenarioCondition.EntityState -> {
            val name = entryTitle(condition.entityId) ?: condition.entityId
            context.getString(R.string.scenarios_condition_entity, name, condition.state)
        }
        is ScenarioCondition.TimeWindow -> {
            val days = if (condition.weekdays.isEmpty()) {
                context.getString(R.string.scenarios_condition_every_day)
            } else {
                dayNames(context, condition.weekdays)
            }
            context.getString(
                R.string.scenarios_condition_window,
                condition.start.display(),
                condition.end.display(),
                days,
            )
        }
        is ScenarioCondition.Unknown -> context.getString(R.string.scenarios_condition_unsupported)
    }

    fun describeStep(
        context: Context,
        step: ScenarioStep,
        entryTitle: (String) -> String?,
    ): String = when (step) {
        is ScenarioStep.Catalog -> {
            val name = entryTitle(step.entryId) ?: step.entryId
            context.getString(R.string.scenarios_step_catalog, actionLabel(context, step.action), name)
        }
        is ScenarioStep.Speak -> context.getString(R.string.scenarios_step_speak, step.text)
        is ScenarioStep.Delay -> context.getString(R.string.scenarios_step_delay, step.seconds)
        is ScenarioStep.Unknown -> context.getString(R.string.scenarios_step_unsupported)
    }

    fun actionLabel(context: Context, action: String): String = when (action) {
        "run" -> context.getString(R.string.scenarios_action_run)
        "turn_on" -> context.getString(R.string.scenarios_action_turn_on)
        "turn_off" -> context.getString(R.string.scenarios_action_turn_off)
        "set" -> context.getString(R.string.scenarios_action_set)
        "speak" -> context.getString(R.string.scenarios_action_speak)
        else -> action
    }

    fun dayNames(context: Context, days: List<Int>): String {
        val labels = context.resources.getStringArray(R.array.scenarios_day_short)
        return days.sorted().joinToString(", ") { labels.getOrElse(it - 1) { it.toString() } }
    }

    fun shortDate(): DateTimeFormatter =
        DateTimeFormatter.ofPattern("d MMM yyyy", Locale.getDefault())

    private fun dev.proflyder.home.core.model.WallTime.display(): String =
        String.format(Locale.ROOT, "%02d:%02d", hour, minute)
}

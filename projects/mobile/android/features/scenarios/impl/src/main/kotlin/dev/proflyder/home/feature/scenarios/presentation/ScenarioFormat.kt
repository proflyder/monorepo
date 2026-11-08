package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.feature.scenarios.ScenarioTriggerStrings
import dev.proflyder.home.feature.scenarios.describeTriggers
import dev.proflyder.home.feature.scenarios.impl.R
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

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

/** Two lines of a step card: a human-readable title and an actionable details line. */
internal data class StepDescription(val title: String, val details: String)

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

    /**
     * Step card text. The title is the catalog entry title (or its id when the
     * entry is gone); details carry the localized action, room/device and a
     * short parameter summary.
     */
    fun describeStep(
        context: Context,
        step: ScenarioStep,
        catalog: List<ControlEntry>,
        catalogLoaded: Boolean,
    ): StepDescription = when (step) {
        is ScenarioStep.Catalog -> {
            val entry = catalog.firstOrNull { it.id == step.entryId }
            val title = entry?.let { it.title.ifBlank { it.id } } ?: step.entryId
            val parts = mutableListOf(actionLabel(context, step.action))
            if (entry != null) {
                parts += listOf(entry.room, entry.device)
                    .filter { it.isNotBlank() && it != title }
                    .distinct()
            } else if (catalogLoaded) {
                parts += context.getString(R.string.scenarios_step_missing)
            }
            parts += paramsSummary(context, step.paramsJson)
            StepDescription(title, parts.joinToString(SEPARATOR))
        }
        is ScenarioStep.Speak -> StepDescription(
            context.getString(R.string.scenarios_step_speak_title),
            context.getString(R.string.scenarios_step_speak_detail, step.text),
        )
        is ScenarioStep.Delay -> StepDescription(
            context.getString(R.string.scenarios_step_delay_title),
            context.getString(R.string.scenarios_step_delay, step.seconds),
        )
        is ScenarioStep.Unknown -> StepDescription(
            context.getString(R.string.scenarios_step_unsupported),
            "",
        )
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

    /** Parsed light/speak parameters in a stable, human-readable order. */
    private fun paramsSummary(context: Context, raw: String): List<String> {
        val obj = try {
            Json.parseToJsonElement(raw.ifBlank { "{}" }) as? JsonObject
        } catch (_: Exception) {
            null
        } ?: return emptyList()
        val parts = mutableListOf<String>()
        obj.int("brightness")?.let { parts += context.getString(R.string.scenarios_param_brightness, it) }
        obj.int("kelvin")?.let { parts += context.getString(R.string.scenarios_param_kelvin, it) }
        obj.string("effect")?.let { parts += context.getString(R.string.scenarios_param_effect, it) }
        val hue = obj.int("hue")
        val saturation = obj.int("saturation")
        if (hue != null && saturation != null) {
            parts += context.getString(R.string.scenarios_param_color, hue, saturation)
        }
        obj.double("transition")?.let {
            parts += context.getString(R.string.scenarios_param_transition, decimal(it))
        }
        obj.string("text")?.let { parts += context.getString(R.string.scenarios_param_text, it) }
        return parts
    }

    private fun JsonObject.string(name: String): String? =
        (this[name] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() }

    private fun JsonObject.int(name: String): Int? =
        (this[name] as? JsonPrimitive)?.contentOrNull?.toIntOrNull()

    private fun JsonObject.double(name: String): Double? =
        (this[name] as? JsonPrimitive)?.contentOrNull?.toDoubleOrNull()

    private fun decimal(value: Double): String =
        DecimalFormat("0.##", DecimalFormatSymbols(Locale.getDefault())).format(value)

    private fun dev.proflyder.home.core.model.WallTime.display(): String =
        String.format(Locale.ROOT, "%02d:%02d", hour, minute)

    /** Separator between the action, room/device and parameter summaries. */
    const val SEPARATOR = " · "
}

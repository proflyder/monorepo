package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioTrigger
import java.time.LocalDate

/**
 * Localized label sources for scenario summaries. The API stays resource-free;
 * the implementation supplies strings from `values/` and `values-ru/`.
 */
interface ScenarioTriggerStrings {
    fun manual(): String
    fun daily(): String
    fun weekdays(): String
    fun weekends(): String
    fun once(date: LocalDate?): String
    fun custom(days: List<Int>): String
    fun unsupported(): String

    /** `07:30`; formatting itself is locale-independent. */
    fun time(hour: Int, minute: Int): String
}

/**
 * Short trigger summary for a scenario card, for example `Weekdays 07:30`.
 *
 * Manual is implicit and shown only when the scenario has no time triggers, so
 * a "manual + daily" definition reads as `Every day 07:30`, not duplicated.
 */
fun describeTriggers(triggers: List<ScenarioTrigger>, strings: ScenarioTriggerStrings): String {
    val parts = mutableListOf<String>()
    var hasUnsupported = false
    triggers.forEach { trigger ->
        when (trigger) {
            ScenarioTrigger.Manual -> Unit
            is ScenarioTrigger.Time -> {
                val time = trigger.trigger
                val repeat = when (time.repeat) {
                    RepeatKind.ONCE -> strings.once(
                        runCatching { LocalDate.parse(time.onceDate) }.getOrNull(),
                    )
                    RepeatKind.DAILY -> strings.daily()
                    RepeatKind.WEEKDAYS -> strings.weekdays()
                    RepeatKind.WEEKENDS -> strings.weekends()
                    RepeatKind.CUSTOM -> strings.custom(time.weekdays)
                    else -> strings.unsupported()
                }
                parts += "$repeat ${strings.time(time.at.hour, time.at.minute)}"
            }
            is ScenarioTrigger.Unknown -> hasUnsupported = true
        }
    }
    if (hasUnsupported) parts += strings.unsupported()
    if (parts.isEmpty()) return strings.manual()
    return parts.joinToString(", ")
}

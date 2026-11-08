package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioLimits
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.time.TimeText
import dev.proflyder.home.core.time.Weekdays
import dev.proflyder.home.core.text.SpeakText
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.serialization.json.Json

/**
 * Local mirror of the scheduler's scenario validation. The server remains the
 * authority; this keeps Save disabled for input the server would reject and
 * gives the user a concrete field instead of a generic server error.
 */
object ScenarioValidation {
    private val supportedRepeats = setOf(
        RepeatKind.ONCE,
        RepeatKind.DAILY,
        RepeatKind.WEEKDAYS,
        RepeatKind.WEEKENDS,
        RepeatKind.CUSTOM,
    )

    private val entityIdPattern = Regex("^[a-z][a-z0-9_]*\\.[a-z][a-z0-9_]*$")

    fun validate(draft: ScenarioDraft): RemoteFailure? {
        if (draft.triggers.any { !it.isSupported } ||
            draft.conditions.any { !it.isSupported } ||
            draft.steps.any { !it.isSupported }
        ) {
            return RemoteFailure.Validation(Field.UNSUPPORTED)
        }

        val label = draft.label.trim()
        if (label.isEmpty() || label.toByteArray(Charsets.UTF_8).size > ScenarioLimits.MAX_LABEL_BYTES) {
            return RemoteFailure.Validation(Field.LABEL)
        }
        if (draft.description.toByteArray(Charsets.UTF_8).size > ScenarioLimits.MAX_DESCRIPTION_BYTES) {
            return RemoteFailure.Validation(Field.DESCRIPTION)
        }
        if (draft.triggers.size > ScenarioLimits.MAX_TRIGGERS) {
            return RemoteFailure.Validation(Field.TRIGGERS)
        }
        if (draft.conditions.size > ScenarioLimits.MAX_CONDITIONS) {
            return RemoteFailure.Validation(Field.CONDITIONS)
        }
        if (draft.steps.size > ScenarioLimits.MAX_STEPS) {
            return RemoteFailure.Validation(Field.STEPS)
        }

        draft.triggers.forEach { trigger ->
            triggerProblem(trigger)?.let { return it }
        }
        draft.conditions.forEach { condition ->
            conditionProblem(condition)?.let { return it }
        }
        draft.steps.forEach { step ->
            stepProblem(step)?.let { return it }
        }
        return null
    }

    private fun triggerProblem(trigger: ScenarioTrigger): RemoteFailure? {
        val time = (trigger as? ScenarioTrigger.Time)?.trigger ?: return null
        if (time.at.hour !in 0..23 || time.at.minute !in 0..59) {
            return RemoteFailure.Validation(Field.TIME)
        }
        val zone = when (val parsed = TimeText.parseZone(time.timezone)) {
            is TimeText.ParseResult.Valid -> parsed.value
            is TimeText.ParseResult.Invalid -> return RemoteFailure.Validation(Field.TIMEZONE)
        }
        if (time.repeat !in supportedRepeats) return RemoteFailure.Validation(Field.REPEAT)
        if (time.repeat == RepeatKind.CUSTOM && !Weekdays.isValid(time.weekdays)) {
            return RemoteFailure.Validation(Field.WEEKDAYS)
        }
        if (time.repeat == RepeatKind.ONCE) {
            val date = runCatching { LocalDate.parse(time.onceDate) }.getOrNull()
                ?: return RemoteFailure.Validation(Field.DATE)
            if (date.isBefore(LocalDate.now(ZoneId.of(zone.id)))) {
                return RemoteFailure.Validation(Field.DATE)
            }
        }
        return null
    }

    private fun conditionProblem(condition: ScenarioCondition): RemoteFailure? = when (condition) {
        is ScenarioCondition.EntityState -> {
            if (!entityIdPattern.matches(condition.entityId) ||
                condition.entityId.toByteArray(Charsets.UTF_8).size > ScenarioLimits.MAX_ENTITY_ID_BYTES
            ) {
                RemoteFailure.Validation(Field.ENTITY)
            } else if (condition.state.isEmpty() ||
                condition.state.toByteArray(Charsets.UTF_8).size > ScenarioLimits.MAX_ENTITY_STATE_BYTES ||
                condition.state.any { it.isISOControl() }
            ) {
                RemoteFailure.Validation(Field.ENTITY)
            } else {
                null
            }
        }
        is ScenarioCondition.TimeWindow -> {
            if (condition.start.hour !in 0..23 || condition.start.minute !in 0..59 ||
                condition.end.hour !in 0..23 || condition.end.minute !in 0..59
            ) {
                RemoteFailure.Validation(Field.TIME)
            } else if (condition.weekdays.isNotEmpty() && !Weekdays.isValid(condition.weekdays)) {
                RemoteFailure.Validation(Field.WEEKDAYS)
            } else {
                null
            }
        }
        is ScenarioCondition.Unknown -> RemoteFailure.Validation(Field.UNSUPPORTED)
    }

    private fun stepProblem(step: ScenarioStep): RemoteFailure? = when (step) {
        is ScenarioStep.Catalog -> {
            if (step.entryId.isBlank() ||
                step.entryId.toByteArray(Charsets.UTF_8).size > ScenarioLimits.MAX_ENTRY_ID_BYTES
            ) {
                RemoteFailure.Validation(Field.ENTITY)
            } else if (step.action.isBlank() ||
                step.action.toByteArray(Charsets.UTF_8).size > ScenarioLimits.MAX_ACTION_BYTES
            ) {
                RemoteFailure.Validation(Field.STEPS)
            } else if (step.paramsJson.isNotBlank() && !isJsonObject(step.paramsJson)) {
                RemoteFailure.Validation(Field.STEPS)
            } else {
                null
            }
        }
        is ScenarioStep.Speak -> {
            if (SpeakText.normalize(step.text) == null) {
                RemoteFailure.Validation(Field.SPEAK)
            } else {
                null
            }
        }
        is ScenarioStep.Delay -> {
            if (step.seconds !in ScenarioLimits.MIN_DELAY_SECONDS..ScenarioLimits.MAX_DELAY_SECONDS) {
                RemoteFailure.Validation(Field.DURATION)
            } else {
                null
            }
        }
        is ScenarioStep.Unknown -> RemoteFailure.Validation(Field.UNSUPPORTED)
    }

    private fun isJsonObject(raw: String): Boolean = try {
        Json.parseToJsonElement(raw) is kotlinx.serialization.json.JsonObject
    } catch (_: Exception) {
        false
    }
}

package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.designsystem.LightParamsEditorState
import dev.proflyder.home.core.designsystem.isValid
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.LightRanges
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioLimits
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.text.SpeakText
import dev.proflyder.home.feature.scenarios.data.CatalogStepParams
import java.time.LocalDate
import java.util.Locale

internal sealed interface EditorForm {
    val target: EditTarget
    val isValid: Boolean

    fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft
}

internal data class CatalogAction(val entryId: String, val action: String)

internal data class SpeakForm(
    override val target: EditTarget,
    val text: String,
    val catalog: CatalogAction? = null,
) : EditorForm {
    val remaining: Int get() = SpeakText.remaining(text)

    override val isValid: Boolean get() = SpeakText.normalize(text) != null

    override fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft {
        val spoken = SpeakText.normalize(text) ?: return draft
        val step = catalog?.let { ScenarioStep.Catalog(it.entryId, it.action, CatalogStepParams.speak(spoken)) }
            ?: ScenarioStep.Speak(spoken)
        return draft.withStep(target, step)
    }
}

internal data class DelayForm(
    override val target: EditTarget,
    val amount: String,
    val minutes: Boolean,
) : EditorForm {
    val seconds: Int?
        get() = amount.trim().toIntOrNull()
            ?.let { if (minutes) it * SECONDS_PER_MINUTE else it }
            ?.takeIf { it in ScenarioLimits.MIN_DELAY_SECONDS..ScenarioLimits.MAX_DELAY_SECONDS }

    override val isValid: Boolean get() = seconds != null

    override fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft =
        seconds?.let { draft.withStep(target, ScenarioStep.Delay(it)) } ?: draft

    companion object {
        private const val SECONDS_PER_MINUTE = 60
        const val DEFAULT_SECONDS = 60

        fun of(target: EditTarget, initialSeconds: Int): DelayForm {
            val seconds = initialSeconds.coerceIn(ScenarioLimits.MIN_DELAY_SECONDS, ScenarioLimits.MAX_DELAY_SECONDS)
            val minutes = seconds % SECONDS_PER_MINUTE == 0
            val amount = if (minutes) seconds / SECONDS_PER_MINUTE else seconds
            return DelayForm(target, String.format(Locale.ROOT, "%d", amount), minutes)
        }
    }
}

internal data class EntityConditionForm(
    override val target: EditTarget,
    val entityId: String,
    val state: String,
) : EditorForm {
    override val isValid: Boolean get() = entityId.isNotBlank() && state.isNotBlank()

    override fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft =
        draft.withCondition(target, ScenarioCondition.EntityState(entityId, state))

    companion object {
        const val STATE_ON = "on"
        const val STATE_OFF = "off"
    }
}

internal data class TimeTriggerForm(
    override val target: EditTarget,
    val zone: String,
    val at: WallTime,
    val repeat: RepeatKind,
    val onceDate: LocalDate,
    val weekdays: Set<Int>,
    val today: LocalDate,
) : EditorForm {
    override val isValid: Boolean
        get() = when (repeat) {
            RepeatKind.CUSTOM -> weekdays.isNotEmpty()
            RepeatKind.ONCE -> !onceDate.isBefore(today)
            else -> true
        }

    override fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft = draft.withTrigger(
        target,
        ScenarioTrigger.Time(
            TimeTrigger(
                timezone = zone,
                at = at,
                repeat = repeat,
                onceDate = if (repeat == RepeatKind.ONCE) onceDate.toString() else "",
                weekdays = if (repeat == RepeatKind.CUSTOM) weekdays.sorted() else emptyList(),
            ),
        ),
    )

    fun toggledDay(day: Int, checked: Boolean): TimeTriggerForm =
        copy(weekdays = if (checked) weekdays + day else weekdays - day)
}

internal data class TimeWindowForm(
    override val target: EditTarget,
    val start: WallTime,
    val end: WallTime,
    val weekdays: Set<Int>,
) : EditorForm {
    override val isValid: Boolean get() = true

    override fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft =
        draft.withCondition(target, ScenarioCondition.TimeWindow(start, end, weekdays.sorted()))

    fun toggledDay(day: Int, checked: Boolean): TimeWindowForm =
        copy(weekdays = if (checked) weekdays + day else weekdays - day)
}

internal data class LightForm(
    override val target: EditTarget,
    val catalog: CatalogAction,
    val editor: LightParamsEditorState,
) : EditorForm {
    override val isValid: Boolean get() = editor.isValid

    override fun applyTo(draft: ScenarioEditorDraft): ScenarioEditorDraft = draft.withStep(
        target,
        ScenarioStep.Catalog(catalog.entryId, catalog.action, CatalogStepParams.light(editor.params)),
    )

    companion object {
        const val ACTION_TURN_OFF = "turn_off"
        val LIGHT_ACTIONS = setOf("set", "turn_on", ACTION_TURN_OFF)

        fun of(target: EditTarget, entry: ControlEntry, action: String, paramsJson: String): LightForm? {
            val controls = entry.light ?: return null
            val saved = CatalogStepParams.lightParams(paramsJson)
            val ranges = LightRanges.of(controls).takeIf { action != ACTION_TURN_OFF }
            val effects = if (ranges == null) emptyList() else controls.effects
            val params = saved.copy(
                brightnessPct = ranges?.brightnessOf(saved.brightnessPct, controls.brightnessPct),
                kelvin = ranges?.kelvinOf(saved.kelvin, controls.kelvin),
                effect = saved.effect?.takeIf { it in effects },
            )
            val transition = saved.transitionSeconds?.let { formatTransition(it) }.orEmpty()
            return LightForm(target, CatalogAction(entry.id, action), LightParamsEditorState(ranges, effects, params, transition))
        }

        private fun formatTransition(seconds: Double): String =
            if (seconds % 1.0 == 0.0) seconds.toLong().toString() else seconds.toString()
    }
}

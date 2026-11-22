package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioLimits
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger

internal sealed interface EditTarget {
    data object New : EditTarget

    data class At(val index: Int) : EditTarget
}

internal fun <T> List<T>.placed(target: EditTarget, item: T, limit: Int): List<T> = when (target) {
    EditTarget.New -> if (size >= limit) this else this + item
    is EditTarget.At -> if (target.index in indices) toMutableList().also { it[target.index] = item } else this
}

internal fun <T> List<T>.without(index: Int): List<T> = filterIndexed { position, _ -> position != index }

internal fun <T> List<T>.moved(index: Int, delta: Int): List<T> {
    val target = index + delta
    if (index !in indices || target !in indices) return this
    return toMutableList().also { it.add(target, it.removeAt(index)) }
}

internal fun ScenarioEditorDraft.withTrigger(target: EditTarget, trigger: ScenarioTrigger) =
    copy(triggers = triggers.placed(target, trigger, ScenarioLimits.MAX_TRIGGERS))

internal fun ScenarioEditorDraft.withCondition(target: EditTarget, condition: ScenarioCondition) =
    copy(conditions = conditions.placed(target, condition, ScenarioLimits.MAX_CONDITIONS))

internal fun ScenarioEditorDraft.withStep(target: EditTarget, step: ScenarioStep) =
    copy(steps = steps.placed(target, step, ScenarioLimits.MAX_STEPS))

internal enum class DraftSection {
    TRIGGERS,
    CONDITIONS,
    STEPS,
}

internal fun ScenarioEditorDraft.removed(section: DraftSection, index: Int): ScenarioEditorDraft = when (section) {
    DraftSection.TRIGGERS -> copy(triggers = triggers.without(index))
    DraftSection.CONDITIONS -> copy(conditions = conditions.without(index))
    DraftSection.STEPS -> copy(steps = steps.without(index))
}

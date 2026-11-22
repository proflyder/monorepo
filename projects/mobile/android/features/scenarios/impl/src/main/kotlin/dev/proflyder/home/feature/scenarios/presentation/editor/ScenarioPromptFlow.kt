package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.scenarios.data.CatalogStepParams
import dev.proflyder.home.feature.scenarios.impl.R
import java.time.LocalDate
import java.time.ZoneId

internal interface EditorStateHost {
    val current: ScenarioEditorUiState

    fun update(reducer: ScenarioEditorUiState.() -> ScenarioEditorUiState)

    fun editDraft(transform: (ScenarioEditorDraft) -> ScenarioEditorDraft)

    fun notify(event: ScenarioEditorEvent)
}

internal class ScenarioPromptFlow(
    private val host: EditorStateHost,
    private val zone: () -> ZoneId = ZoneId::systemDefault,
) {
    fun open(form: EditorForm) = host.update { copy(editor = form) }

    fun prompt(prompt: ScenarioPrompt?) = host.update { copy(prompt = prompt) }

    fun addTimeTrigger() = open(timeTriggerForm(EditTarget.New, null))

    fun addTimeWindow() = open(TimeWindowForm(EditTarget.New, WallTime(8, 0), WallTime(22, 0), emptySet()))

    fun edit(section: DraftSection, index: Int) {
        val draft = host.current.draft
        val target = EditTarget.At(index)
        when (section) {
            DraftSection.TRIGGERS -> (draft.triggers.getOrNull(index) as? ScenarioTrigger.Time)
                ?.let { open(timeTriggerForm(target, it.trigger)) }
            DraftSection.CONDITIONS -> draft.conditions.getOrNull(index)?.let { editCondition(target, it) }
            DraftSection.STEPS -> draft.steps.getOrNull(index)?.let { editStep(target, it) }
        }
    }

    fun updateForm(form: EditorForm) = host.update {
        if (editor?.target == form.target && editor::class == form::class) copy(editor = form) else this
    }

    fun commitForm() {
        val form = host.current.editor?.takeIf { it.isValid } ?: return
        host.update { copy(editor = null) }
        host.editDraft(form::applyTo)
    }

    fun entryPicked(entryId: String) {
        val picker = host.current.prompt as? ScenarioPrompt.PickEntry ?: return
        val entry = host.current.catalog.entries.firstOrNull { it.id == entryId } ?: return
        prompt(null)
        when (val purpose = picker.purpose) {
            is EntryPurpose.Step -> chooseAction(entry, purpose)
            EntryPurpose.NewCondition -> open(EntityConditionForm(EditTarget.New, entry.id, EntityConditionForm.STATE_ON))
            EntryPurpose.ConditionEntity -> (host.current.editor as? EntityConditionForm)
                ?.let { open(it.copy(entityId = entry.id)) }
        }
    }

    fun showSettingsEntries(visible: Boolean) = host.update {
        copy(prompt = (prompt as? ScenarioPrompt.PickEntry)?.copy(includeSettings = visible) ?: prompt)
    }

    fun actionChosen(action: String) {
        val choice = host.current.prompt as? ScenarioPrompt.ChooseAction ?: return
        prompt(null)
        proceed(choice.entry, action, choice.purpose)
    }

    private fun editCondition(target: EditTarget, condition: ScenarioCondition) = when (condition) {
        is ScenarioCondition.EntityState -> open(EntityConditionForm(target, condition.entityId, condition.state))
        is ScenarioCondition.TimeWindow ->
            open(TimeWindowForm(target, condition.start, condition.end, condition.weekdays.filter(::isWeekday).toSet()))
        is ScenarioCondition.Unknown -> Unit
    }

    private fun editStep(target: EditTarget, step: ScenarioStep) = when (step) {
        is ScenarioStep.Catalog -> prompt(
            ScenarioPrompt.PickEntry(EntryPurpose.Step(target, step.action, step.paramsJson), step.entryId),
        )
        is ScenarioStep.Speak -> open(SpeakForm(target, step.text))
        is ScenarioStep.Delay -> open(DelayForm.of(target, step.seconds))
        is ScenarioStep.Unknown -> Unit
    }

    private fun chooseAction(entry: ControlEntry, purpose: EntryPurpose.Step) {
        val actions = entry.actions.map { it.wire }.sorted()
        when (actions.size) {
            0 -> host.notify(ScenarioEditorEvent.Error(R.string.scenarios_entry_has_no_actions))
            1 -> proceed(entry, actions.single(), purpose)
            else -> prompt(
                ScenarioPrompt.ChooseAction(entry, actions.sortedByDescending { it == purpose.initialAction }, purpose),
            )
        }
    }

    private fun proceed(entry: ControlEntry, action: String, purpose: EntryPurpose.Step) {
        val catalog = CatalogAction(entry.id, action)
        val light = LightForm.of(purpose.target, entry, action, purpose.paramsJson).takeIf { action in LightForm.LIGHT_ACTIONS }
        when {
            action == ACTION_SPEAK -> open(SpeakForm(purpose.target, CatalogStepParams.speakText(purpose.paramsJson), catalog))
            light != null -> open(light)
            else -> host.editDraft { it.withStep(purpose.target, ScenarioStep.Catalog(entry.id, action, CatalogStepParams.EMPTY)) }
        }
    }

    private fun timeTriggerForm(target: EditTarget, initial: TimeTrigger?): TimeTriggerForm {
        val zoneId = runCatching { ZoneId.of(initial?.timezone.orEmpty()) }.getOrDefault(zone())
        val today = LocalDate.now(zoneId)
        return TimeTriggerForm(
            target = target,
            zone = zoneId.id,
            at = initial?.at ?: DEFAULT_TRIGGER_TIME,
            repeat = initial?.repeat?.takeIf { it != RepeatKind.UNSPECIFIED } ?: RepeatKind.DAILY,
            onceDate = runCatching { LocalDate.parse(initial?.onceDate.orEmpty()) }.getOrNull() ?: today,
            weekdays = initial?.weekdays.orEmpty().filter(::isWeekday).toSet(),
            today = today,
        )
    }

    private fun isWeekday(day: Int): Boolean = day in 1..7

    companion object {
        const val ACTION_SPEAK = "speak"
        private val DEFAULT_TRIGGER_TIME = WallTime(7, 30)
    }
}

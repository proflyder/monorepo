package dev.proflyder.home.feature.scenarios.presentation.editor

import android.content.Context
import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.scenarios.describeTriggers
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenarioEditorBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioConditionBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioStepBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioTriggerBinding
import dev.proflyder.home.feature.scenarios.presentation.ResourceTriggerStrings
import dev.proflyder.home.feature.scenarios.presentation.ScenarioFormat
import dev.proflyder.home.feature.scenarios.presentation.titleOf

internal class ScenarioDraftSections(
    private val binding: FragmentScenarioEditorBinding,
    private val dispatch: (ScenarioEditorIntent) -> Unit,
) {
    private val context: Context get() = binding.root.context
    private val inflater: LayoutInflater get() = LayoutInflater.from(context)

    fun render(draft: ScenarioEditorDraft, catalog: CatalogState) {
        binding.scenarioEditorTriggers.rebuild(draft.triggers) { index, trigger -> triggerRow(index, trigger) }
        binding.scenarioEditorConditions.rebuild(draft.conditions) { index, condition -> conditionRow(index, condition, catalog.entries) }
        binding.scenarioEditorSteps.rebuild(draft.steps) { index, step -> stepRow(index, step, draft.steps.lastIndex, catalog) }
    }

    private fun <T> ViewGroup.rebuild(items: List<T>, row: ViewGroup.(Int, T) -> Unit) {
        removeAllViews()
        items.forEachIndexed { index, item -> row(index, item) }
    }

    private fun ViewGroup.triggerRow(index: Int, trigger: ScenarioTrigger) {
        if (trigger is ScenarioTrigger.Manual) return
        val item = ItemScenarioTriggerBinding.inflate(inflater, this, false)
        item.triggerSummary.text = when (trigger) {
            is ScenarioTrigger.Time -> describeTriggers(listOf(trigger), ResourceTriggerStrings(context))
            else -> context.getString(R.string.scenarios_trigger_unsupported)
        }
        item.triggerEdit.setVisible(trigger is ScenarioTrigger.Time)
        item.triggerEdit.setOnClickListener { dispatch(ScenarioEditorIntent.Edit(DraftSection.TRIGGERS, index)) }
        item.triggerDelete.setOnClickListener { dispatch(ScenarioEditorIntent.Remove(DraftSection.TRIGGERS, index)) }
        addView(item.root)
    }

    private fun ViewGroup.conditionRow(index: Int, condition: ScenarioCondition, catalog: List<ControlEntry>) {
        val item = ItemScenarioConditionBinding.inflate(inflater, this, false)
        item.conditionSummary.text = ScenarioFormat.describeCondition(context, condition) { id -> catalog.titleOf(id) }
        item.conditionEdit.setVisible(condition !is ScenarioCondition.Unknown)
        item.conditionEdit.setOnClickListener { dispatch(ScenarioEditorIntent.Edit(DraftSection.CONDITIONS, index)) }
        item.conditionDelete.setOnClickListener { dispatch(ScenarioEditorIntent.Remove(DraftSection.CONDITIONS, index)) }
        addView(item.root)
    }

    private fun ViewGroup.stepRow(index: Int, step: ScenarioStep, lastIndex: Int, catalog: CatalogState) {
        val item = ItemScenarioStepBinding.inflate(inflater, this, false)
        val description = ScenarioFormat.describeStep(context, step, catalog.entries, catalog.loaded)
        item.stepIndex.text = context.getString(R.string.scenarios_step_number, index + 1)
        item.stepSummary.text = description.title
        item.stepDetails.text = description.details
        item.stepDetails.setVisible(description.details.isNotBlank())
        item.stepUp.isEnabled = index > 0
        item.stepDown.isEnabled = index < lastIndex
        item.stepUp.setOnClickListener { dispatch(ScenarioEditorIntent.MoveStep(index, -1)) }
        item.stepDown.setOnClickListener { dispatch(ScenarioEditorIntent.MoveStep(index, 1)) }
        item.stepEdit.setVisible(step !is ScenarioStep.Unknown)
        item.stepEdit.setOnClickListener { dispatch(ScenarioEditorIntent.Edit(DraftSection.STEPS, index)) }
        item.stepDelete.setOnClickListener { dispatch(ScenarioEditorIntent.Remove(DraftSection.STEPS, index)) }
        addView(item.root)
    }
}

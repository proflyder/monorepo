package dev.proflyder.home.feature.scenarios.presentation

import android.os.Bundle
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioLimits
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.scenarios.ScenarioEditorCompletion
import dev.proflyder.home.feature.scenarios.ScenarioEditorDestination
import dev.proflyder.home.feature.scenarios.describeTriggers
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenarioEditorBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioConditionBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioStepBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioTriggerBinding
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

class ScenarioEditorFragment :
    BaseFragment<FragmentScenarioEditorBinding>(FragmentScenarioEditorBinding.Companion::inflate) {
    private val viewModel: ScenarioEditorViewModel by viewModels {
        viewModelFactory(ScenarioEditorViewModel::class.java)
    }

    private var rendering = false
    private var conflictDialogShown = false

    override fun onBindingCreated(binding: FragmentScenarioEditorBinding, savedInstanceState: Bundle?) {
        val scenarioId = arguments?.getString(ScenarioEditorDestination.ARG_SCENARIO_ID)
        viewModel.start(scenarioId)
        binding.scenarioEditorDelete.setVisible(scenarioId != null)

        binding.scenarioEditorLabel.doAfterTextChanged {
            if (!rendering) viewModel.setLabel(it?.toString().orEmpty())
        }
        binding.scenarioEditorDescription.doAfterTextChanged {
            if (!rendering) viewModel.setDescription(it?.toString().orEmpty())
        }
        binding.scenarioEditorEnabled.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setEnabled(checked)
        }
        binding.scenarioEditorContinueOnError.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setContinueOnError(checked)
        }

        binding.scenarioEditorAddTrigger.setOnClickListener {
            TimeTriggerDialog(
                requireContext(),
                childFragmentManager,
                initial = null,
                onSave = viewModel::addTimeTrigger,
            ).show()
        }
        binding.scenarioEditorAddConditionEntity.setOnClickListener {
            pickEntry { entry ->
                EntityConditionDialog(
                    requireContext(),
                    initialEntityId = entry.id,
                    initialState = "on",
                    entryTitle = { id -> viewModel.state.value.catalog.titleOf(id) },
                    onPickEntity = { onPicked -> pickEntry { picked -> onPicked(picked.id) } },
                    onSave = { entityId, state ->
                        viewModel.addCondition(ScenarioCondition.EntityState(entityId, state))
                    },
                ).show()
            }
        }
        binding.scenarioEditorAddConditionWindow.setOnClickListener {
            TimeWindowDialog(
                requireContext(),
                childFragmentManager,
                initial = ScenarioEditorViewModel.defaultTimeWindow(),
                onSave = viewModel::addCondition,
            ).show()
        }
        binding.scenarioEditorAddAction.setOnClickListener { pickEntry { entry -> startCatalogStep(entry) } }
        binding.scenarioEditorAddSpeak.setOnClickListener {
            SpeakStepDialog(requireContext(), initial = "") { text ->
                viewModel.addStep(ScenarioStep.Speak(text))
            }.show()
        }
        binding.scenarioEditorAddDelay.setOnClickListener {
            DelayStepDialog(requireContext(), initialSeconds = 60) { seconds ->
                viewModel.addStep(ScenarioStep.Delay(seconds))
            }.show()
        }
        binding.scenarioEditorSave.setOnClickListener { viewModel.save() }
        binding.scenarioEditorDelete.setOnClickListener { confirmDelete() }
        binding.scenarioEditorCatalogStatus.setOnClickListener { viewModel.reloadCatalog() }

        viewModel.state.observe { render(it) }
        viewModel.completion.observe(Lifecycle.State.RESUMED) { completion ->
            if (completion == null) return@observe
            viewModel.consumeCompletion()
            showMessage(
                when (completion) {
                    ScenarioEditorCompletion.SAVED -> R.string.scenarios_saved
                    ScenarioEditorCompletion.DELETED -> R.string.scenarios_deleted
                },
            )
            requireNavigator().back()
        }
    }

    private fun render(state: ScenarioEditorUiState) = with(binding) {
        rendering = true
        val draft = state.draft
        if (scenarioEditorLabel.text?.toString() != draft.label) scenarioEditorLabel.setText(draft.label)
        if (scenarioEditorDescription.text?.toString() != draft.description) {
            scenarioEditorDescription.setText(draft.description)
        }
        scenarioEditorEnabled.isChecked = draft.enabled
        scenarioEditorContinueOnError.isChecked = draft.continueOnError
        scenarioEditorUnsupported.setVisible(state.hasUnsupportedParts)
        scenarioEditorAddTrigger.isEnabled = draft.triggers.size < ScenarioLimits.MAX_TRIGGERS
        scenarioEditorAddConditionEntity.isEnabled = draft.conditions.size < ScenarioLimits.MAX_CONDITIONS
        scenarioEditorAddConditionWindow.isEnabled = draft.conditions.size < ScenarioLimits.MAX_CONDITIONS
        scenarioEditorAddAction.isEnabled = draft.steps.size < ScenarioLimits.MAX_STEPS
        scenarioEditorAddSpeak.isEnabled = draft.steps.size < ScenarioLimits.MAX_STEPS
        scenarioEditorAddDelay.isEnabled = draft.steps.size < ScenarioLimits.MAX_STEPS

        val idle = !state.saving && !state.loading && !state.loadFailed
        scenarioEditorSave.isEnabled = idle && !state.hasUnsupportedParts
        scenarioEditorDelete.isEnabled = idle
        scenarioEditorDelete.setVisible(state.existing != null || draft.existingId != null)
        scenarioEditorProgress.setVisible(state.saving || state.loading)
        scenarioEditorError.setVisible(state.error != null && !state.conflict)
        scenarioEditorError.text = state.error?.userMessage(requireContext())

        scenarioEditorCatalogStatus.setVisible(state.catalogError != null)
        scenarioEditorCatalogStatus.text = state.catalogError?.let {
            getString(R.string.scenarios_catalog_failed, it.userMessage(requireContext()))
        }

        renderTriggers(draft.triggers)
        renderConditions(draft.conditions)
        renderSteps(draft.steps)
        rendering = false

        if (state.conflict && !conflictDialogShown) {
            conflictDialogShown = true
            showConflictDialog()
        } else if (!state.conflict) {
            conflictDialogShown = false
        }
    }

    private fun renderTriggers(triggers: List<ScenarioTrigger>) = with(binding.scenarioEditorTriggers) {
        removeAllViews()
        triggers.forEachIndexed { index, trigger ->
            if (trigger is ScenarioTrigger.Manual) return@forEachIndexed
            val item = ItemScenarioTriggerBinding.inflate(layoutInflater, this, false)
            item.triggerSummary.text = when (trigger) {
                is ScenarioTrigger.Time -> describeTriggers(
                    listOf(trigger),
                    ResourceTriggerStrings(requireContext()),
                )
                is ScenarioTrigger.Unknown -> getString(R.string.scenarios_trigger_unsupported)
                ScenarioTrigger.Manual -> ""
            }
            item.triggerEdit.setVisible(trigger is ScenarioTrigger.Time)
            item.triggerEdit.setOnClickListener {
                val time = (trigger as? ScenarioTrigger.Time) ?: return@setOnClickListener
                TimeTriggerDialog(
                    requireContext(),
                    childFragmentManager,
                    initial = time.trigger,
                    onSave = { viewModel.updateTimeTrigger(index, it) },
                ).show()
            }
            item.triggerDelete.setOnClickListener { viewModel.removeTrigger(index) }
            addView(item.root)
        }
    }

    private fun renderConditions(conditions: List<ScenarioCondition>) = with(binding.scenarioEditorConditions) {
        removeAllViews()
        conditions.forEachIndexed { index, condition ->
            val item = ItemScenarioConditionBinding.inflate(layoutInflater, this, false)
            item.conditionSummary.text = ScenarioFormat.describeCondition(
                requireContext(),
                condition,
                { id -> viewModel.state.value.catalog.titleOf(id) },
            )
            item.conditionEdit.setVisible(condition !is ScenarioCondition.Unknown)
            item.conditionEdit.setOnClickListener {
                when (condition) {
                    is ScenarioCondition.EntityState -> EntityConditionDialog(
                        requireContext(),
                        initialEntityId = condition.entityId,
                        initialState = condition.state,
                        entryTitle = { id -> viewModel.state.value.catalog.titleOf(id) },
                        onPickEntity = { onPicked -> pickEntry { picked -> onPicked(picked.id) } },
                        onSave = { entityId, state ->
                            viewModel.updateCondition(index, ScenarioCondition.EntityState(entityId, state))
                        },
                    ).show()
                    is ScenarioCondition.TimeWindow -> TimeWindowDialog(
                        requireContext(),
                        childFragmentManager,
                        initial = condition,
                        onSave = { viewModel.updateCondition(index, it) },
                    ).show()
                    is ScenarioCondition.Unknown -> Unit
                }
            }
            item.conditionDelete.setOnClickListener { viewModel.removeCondition(index) }
            addView(item.root)
        }
    }

    private fun renderSteps(steps: List<ScenarioStep>) = with(binding.scenarioEditorSteps) {
        removeAllViews()
        steps.forEachIndexed { index, step ->
            val item = ItemScenarioStepBinding.inflate(layoutInflater, this, false)
            item.stepIndex.text = getString(R.string.scenarios_step_number, index + 1)
            item.stepSummary.text = ScenarioFormat.describeStep(
                requireContext(),
                step,
                { id -> viewModel.state.value.catalog.titleOf(id) },
            )
            item.stepUp.isEnabled = index > 0
            item.stepDown.isEnabled = index < steps.lastIndex
            item.stepUp.setOnClickListener { viewModel.moveStep(index, -1) }
            item.stepDown.setOnClickListener { viewModel.moveStep(index, 1) }
            item.stepEdit.setVisible(step !is ScenarioStep.Unknown)
            item.stepEdit.setOnClickListener { editStep(index, step) }
            item.stepDelete.setOnClickListener { viewModel.removeStep(index) }
            addView(item.root)
        }
    }

    private fun editStep(index: Int, step: ScenarioStep) {
        when (step) {
            is ScenarioStep.Catalog -> pickEntry(initialId = step.entryId) { entry ->
                chooseAction(entry, initialAction = step.action) { action ->
                    withParams(entry, action, step.paramsJson) { params ->
                        viewModel.updateStep(index, ScenarioStep.Catalog(entry.id, action, params))
                    }
                }
            }
            is ScenarioStep.Speak -> SpeakStepDialog(requireContext(), step.text) { text ->
                viewModel.updateStep(index, ScenarioStep.Speak(text))
            }.show()
            is ScenarioStep.Delay -> DelayStepDialog(requireContext(), step.seconds) { seconds ->
                viewModel.updateStep(index, ScenarioStep.Delay(seconds))
            }.show()
            is ScenarioStep.Unknown -> Unit
        }
    }

    private fun startCatalogStep(entry: ControlEntry) {
        chooseAction(entry, initialAction = null) { action ->
            withParams(entry, action, "{}") { params ->
                viewModel.addStep(ScenarioStep.Catalog(entry.id, action, params))
            }
        }
    }

    private fun pickEntry(initialId: String? = null, onSelected: (ControlEntry) -> Unit) {
        val entries = viewModel.state.value.catalog
        if (entries.isEmpty()) {
            viewModel.state.value.catalogError?.let { showFailure(it) }
                ?: showMessage(R.string.scenarios_catalog_empty)
            return
        }
        val ordered = if (initialId == null) {
            entries
        } else {
            entries.sortedByDescending { it.id == initialId }
        }
        CatalogPickerDialog(requireContext(), ordered, onSelected).show()
    }

    private fun chooseAction(entry: ControlEntry, initialAction: String?, onSelected: (String) -> Unit) {
        val actions = entry.actions.map { it.wire }.sorted()
        if (actions.isEmpty()) {
            showMessage(R.string.scenarios_entry_has_no_actions)
            return
        }
        if (actions.size == 1) {
            onSelected(actions.single())
            return
        }
        val labels = actions.map { ScenarioFormat.actionLabel(requireContext(), it) }.toTypedArray()
        val checked = actions.indexOf(initialAction)
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.scenarios_action_dialog_title)
            .setSingleChoiceItems(labels, checked) { dialog, which ->
                dialog.dismiss()
                onSelected(actions[which])
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun withParams(
        entry: ControlEntry,
        action: String,
        initialParamsJson: String,
        onDone: (String) -> Unit,
    ) {
        when {
            action == "speak" -> SpeakStepDialog(requireContext(), initial = initialSpeak(initialParamsJson)) { text ->
                onDone(buildJsonObject { put("text", text) }.toString())
            }.show()
            entry.light != null && action in setOf("set", "turn_on", "turn_off") ->
                LightParamsDialog(requireContext(), entry, action, initialParamsJson, onDone).show()
            else -> onDone("{}")
        }
    }

    private fun initialSpeak(raw: String): String = try {
        val json = kotlinx.serialization.json.Json.parseToJsonElement(raw.ifBlank { "{}" }) as? JsonObject
        json?.get("text")?.jsonPrimitive?.content.orEmpty()
    } catch (_: Exception) {
        ""
    }

    private fun showConflictDialog() {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.scenarios_conflict_title)
            .setMessage(R.string.scenarios_conflict_message)
            .setPositiveButton(R.string.scenarios_conflict_reload) { _, _ -> viewModel.reload() }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun confirmDelete() {
        confirm(R.string.scenarios_delete_confirm, R.string.scenarios_delete_yes, R.string.scenarios_cancel) {
            viewModel.delete()
        }
    }
}

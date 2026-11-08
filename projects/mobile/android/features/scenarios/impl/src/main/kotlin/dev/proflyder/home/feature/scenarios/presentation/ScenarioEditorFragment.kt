package dev.proflyder.home.feature.scenarios.presentation

import androidx.appcompat.app.AlertDialog
import android.os.Bundle
import android.widget.ProgressBar
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

    /** A picker waiting for the catalog to finish loading. */
    private var pendingPick: ((List<ControlEntry>) -> Unit)? = null
    private var catalogWaitDialog: AlertDialog? = null

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

        binding.scenarioEditorAddTrigger.setOnClickListener { showAddTriggerSheet() }
        binding.scenarioEditorAddCondition.setOnClickListener { showAddConditionSheet() }
        binding.scenarioEditorAddStep.setOnClickListener { showAddStepSheet() }
        binding.scenarioEditorSave.setOnClickListener { viewModel.save() }
        binding.scenarioEditorDelete.setOnClickListener { confirmDelete() }
        binding.scenarioEditorCatalogStatus.setOnClickListener { viewModel.reloadCatalog() }

        viewModel.state.observe { render(it) }
        viewModel.completion.observe(Lifecycle.State.RESUMED) { completion -> handleCompletion(completion) }
    }

    private fun handleCompletion(completion: ScenarioEditorCompletion?) {
        if (completion == null) return
        viewModel.consumeCompletion()
        showMessage(
            when (completion) {
                ScenarioEditorCompletion.SAVED -> R.string.scenarios_saved
                ScenarioEditorCompletion.DELETED -> R.string.scenarios_deleted
            },
        )
        requireNavigator().back()
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
        scenarioEditorAddCondition.isEnabled = draft.conditions.size < ScenarioLimits.MAX_CONDITIONS
        scenarioEditorAddStep.isEnabled = draft.steps.size < ScenarioLimits.MAX_STEPS

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
        renderSteps(draft.steps, state.catalog, !state.catalogLoading && state.catalogError == null)
        rendering = false

        when {
            state.conflict && !conflictDialogShown -> {
                conflictDialogShown = true
                showConflictDialog()
            }
            !state.conflict -> conflictDialogShown = false
        }
        drainPendingPick(state)
    }

    private fun renderTriggers(triggers: List<ScenarioTrigger>) = with(binding.scenarioEditorTriggers) {
        removeAllViews()
        triggers.forEachIndexed { index, trigger -> addTrigger(this, index, trigger) }
    }

    private fun addTrigger(container: android.view.ViewGroup, index: Int, trigger: ScenarioTrigger) {
        if (trigger is ScenarioTrigger.Manual) return
        val item = ItemScenarioTriggerBinding.inflate(layoutInflater, container, false)
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
        container.addView(item.root)
    }

    private fun renderConditions(conditions: List<ScenarioCondition>) = with(binding.scenarioEditorConditions) {
        removeAllViews()
        conditions.forEachIndexed { index, condition -> addCondition(this, index, condition) }
    }

    private fun addCondition(
        container: android.view.ViewGroup,
        index: Int,
        condition: ScenarioCondition,
    ) {
        val item = ItemScenarioConditionBinding.inflate(layoutInflater, container, false)
        item.conditionSummary.text = ScenarioFormat.describeCondition(
            requireContext(),
            condition,
            { id -> viewModel.state.value.catalog.titleOf(id) },
        )
        item.conditionEdit.setVisible(condition !is ScenarioCondition.Unknown)
        item.conditionEdit.setOnClickListener {
            when (condition) {
                is ScenarioCondition.EntityState -> showEntityConditionDialog(
                    entityId = condition.entityId,
                    state = condition.state,
                ) { entityId, newState ->
                    viewModel.updateCondition(index, ScenarioCondition.EntityState(entityId, newState))
                }
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
        container.addView(item.root)
    }

    private fun renderSteps(
        steps: List<ScenarioStep>,
        catalog: List<ControlEntry>,
        catalogLoaded: Boolean,
    ) = with(binding.scenarioEditorSteps) {
        removeAllViews()
        steps.forEachIndexed { index, step -> addStep(this, index, step, catalog, catalogLoaded) }
    }

    private fun addStep(
        container: android.view.ViewGroup,
        index: Int,
        step: ScenarioStep,
        catalog: List<ControlEntry>,
        catalogLoaded: Boolean,
    ) {
        val item = ItemScenarioStepBinding.inflate(layoutInflater, container, false)
        val description = ScenarioFormat.describeStep(requireContext(), step, catalog, catalogLoaded)
        item.stepIndex.text = getString(R.string.scenarios_step_number, index + 1)
        item.stepSummary.text = description.title
        item.stepDetails.text = description.details
        item.stepDetails.setVisible(description.details.isNotBlank())
        item.stepUp.isEnabled = index > 0
        item.stepDown.isEnabled = index < steps.lastIndex
        item.stepUp.setOnClickListener { viewModel.moveStep(index, -1) }
        item.stepDown.setOnClickListener { viewModel.moveStep(index, 1) }
        item.stepEdit.setVisible(step !is ScenarioStep.Unknown)
        item.stepEdit.setOnClickListener { editStep(index, step) }
        item.stepDelete.setOnClickListener { viewModel.removeStep(index) }
        container.addView(item.root)
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
        val state = viewModel.state.value
        if (state.catalog.isEmpty()) {
            when {
                state.catalogLoading -> {
                    pendingPick = { entries -> showCatalogPicker(entries, initialId, onSelected) }
                    showCatalogWait()
                }
                state.catalogError != null -> showCatalogError(state.catalogError)
                else -> showError(R.string.scenarios_catalog_empty)
            }
            return
        }
        showCatalogPicker(state.catalog, initialId, onSelected)
    }

    private fun showCatalogPicker(
        entries: List<ControlEntry>,
        initialId: String?,
        onSelected: (ControlEntry) -> Unit,
    ) {
        val ordered = if (initialId == null) {
            entries
        } else {
            entries.sortedByDescending { it.id == initialId }
        }
        CatalogPickerDialog(requireContext(), ordered, onSelected).show()
    }

    /** Dismisses the "catalog is loading" dialog and continues a pending picker. */
    private fun drainPendingPick(state: ScenarioEditorUiState) {
        val pending = pendingPick ?: return
        if (state.catalogLoading) return
        catalogWaitDialog?.dismiss()
        catalogWaitDialog = null
        pendingPick = null
        when {
            state.catalogError != null -> showCatalogError(state.catalogError)
            state.catalog.isNotEmpty() -> pending(state.catalog)
            else -> showError(R.string.scenarios_catalog_empty)
        }
    }

    private fun showCatalogError(failure: dev.proflyder.home.core.model.RemoteFailure) {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.scenarios_catalog_error_title)
            .setMessage(failure.userMessage(requireContext()))
            .setPositiveButton(R.string.scenarios_retry) { _, _ -> viewModel.reloadCatalog() }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun showCatalogWait() {
        if (catalogWaitDialog?.isShowing == true) return
        val progress = ProgressBar(requireContext()).apply {
            isIndeterminate = true
            val padding = (24 * resources.displayMetrics.density).toInt()
            setPadding(padding, padding, padding, padding)
        }
        catalogWaitDialog = MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.scenarios_catalog_loading)
            .setView(progress)
            .setNegativeButton(android.R.string.cancel) { _, _ -> pendingPick = null }
            .show()
    }

    private fun showAddTriggerSheet() {
        ChoiceSheet(
            requireContext(),
            R.string.scenarios_editor_add_trigger_title,
            listOf(
                ChoiceSheet.Choice(
                    label = getString(R.string.scenarios_editor_add_trigger_time),
                    iconRes = R.drawable.scenarios_ic_schedule,
                ) {
                    TimeTriggerDialog(
                        requireContext(),
                        childFragmentManager,
                        initial = null,
                        onSave = viewModel::addTimeTrigger,
                    ).show()
                },
            ),
        ).show()
    }

    private fun showAddConditionSheet() {
        ChoiceSheet(
            requireContext(),
            R.string.scenarios_editor_add_condition_title,
            listOf(
                ChoiceSheet.Choice(
                    label = getString(R.string.scenarios_editor_add_condition_entity),
                    iconRes = R.drawable.scenarios_ic_devices,
                ) {
                    pickEntry { entry ->
                        showEntityConditionDialog(entityId = entry.id, state = "on") { entityId, state ->
                            viewModel.addCondition(ScenarioCondition.EntityState(entityId, state))
                        }
                    }
                },
                ChoiceSheet.Choice(
                    label = getString(R.string.scenarios_editor_add_condition_window),
                    iconRes = R.drawable.scenarios_ic_schedule,
                ) {
                    TimeWindowDialog(
                        requireContext(),
                        childFragmentManager,
                        initial = ScenarioEditorViewModel.defaultTimeWindow(),
                        onSave = viewModel::addCondition,
                    ).show()
                },
            ),
        ).show()
    }

    private fun showAddStepSheet() {
        ChoiceSheet(
            requireContext(),
            R.string.scenarios_editor_add_step_title,
            listOf(
                ChoiceSheet.Choice(
                    label = getString(R.string.scenarios_editor_add_action),
                    iconRes = R.drawable.scenarios_ic_tune,
                ) {
                    pickEntry { entry -> startCatalogStep(entry) }
                },
                ChoiceSheet.Choice(
                    label = getString(R.string.scenarios_editor_add_speak),
                    iconRes = R.drawable.scenarios_ic_speak,
                ) {
                    SpeakStepDialog(requireContext(), initial = "") { text ->
                        viewModel.addStep(ScenarioStep.Speak(text))
                    }.show()
                },
                ChoiceSheet.Choice(
                    label = getString(R.string.scenarios_editor_add_delay),
                    iconRes = R.drawable.scenarios_ic_timer,
                ) {
                    DelayStepDialog(requireContext(), initialSeconds = 60) { seconds ->
                        viewModel.addStep(ScenarioStep.Delay(seconds))
                    }.show()
                },
            ),
        ).show()
    }

    private fun showEntityConditionDialog(
        entityId: String,
        state: String,
        onSave: (String, String) -> Unit,
    ) {
        EntityConditionDialog(
            requireContext(),
            initialEntityId = entityId,
            initialState = state,
            entryTitle = { id -> viewModel.state.value.catalog.titleOf(id) },
            onPickEntity = { onPicked -> pickEntry { picked -> onPicked(picked.id) } },
            onSave = onSave,
        ).show()
    }

    override fun onBindingDestroyed() {
        catalogWaitDialog?.dismiss()
        catalogWaitDialog = null
        pendingPick = null
    }

    private fun chooseAction(entry: ControlEntry, initialAction: String?, onSelected: (String) -> Unit) {
        val actions = entry.actions.map { it.wire }.sorted()
        if (actions.isEmpty()) {
            showError(R.string.scenarios_entry_has_no_actions)
            return
        }
        if (actions.size == 1) {
            onSelected(actions.single())
            return
        }
        val checked = actions.indexOf(initialAction)
        val ordered = if (checked <= 0) actions else actions.sortedByDescending { it == initialAction }
        ChoiceSheet(
            requireContext(),
            R.string.scenarios_action_dialog_title,
            ordered.map { wire ->
                ChoiceSheet.Choice(
                    label = ScenarioFormat.actionLabel(requireContext(), wire),
                    iconRes = if (wire == "speak") R.drawable.scenarios_ic_speak else R.drawable.scenarios_ic_tune,
                ) { onSelected(wire) }
            },
        ).show()
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

    private fun initialSpeak(raw: String): String = runCatching {
        val json = kotlinx.serialization.json.Json.parseToJsonElement(raw.ifBlank { "{}" }) as? JsonObject
        json?.get("text")?.jsonPrimitive?.content.orEmpty()
    }.getOrElse { "" }

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

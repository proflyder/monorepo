package dev.proflyder.home.feature.scenarios.presentation.editor

import android.os.Bundle
import android.widget.ProgressBar
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.DialogFragment
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenarioEditorBinding
import dev.proflyder.home.feature.scenarios.presentation.CatalogPickerDialog
import dev.proflyder.home.feature.scenarios.presentation.ChoiceSheet
import dev.proflyder.home.feature.scenarios.presentation.ScenarioFormat
import dev.proflyder.home.feature.scenarios.presentation.dialog.DelayFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.EntityConditionFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.LightFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.ScenarioFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.SpeakFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.TimeTriggerFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.TimeWindowFormDialog
import dev.proflyder.home.feature.scenarios.presentation.dialog.showText

internal class ScenarioEditorFragment : ScreenFragment<
    FragmentScenarioEditorBinding,
    ScenarioEditorUiState,
    ScenarioEditorIntent,
    ScenarioEditorEvent,
    >(FragmentScenarioEditorBinding.Companion::inflate) {
    override val viewModel: ScenarioEditorViewModel by screenViewModel()

    private val promptSlot = DialogSlot()
    private lateinit var sections: ScenarioDraftSections
    private lateinit var enabled: SwitchBinder
    private lateinit var continueOnError: SwitchBinder

    override fun onScreenCreated(binding: FragmentScenarioEditorBinding, savedInstanceState: Bundle?) = with(binding) {
        root.applyScrollBottomInset()
        root.keepFocusedVisible()
        sections = ScenarioDraftSections(binding, ::dispatch)
        scenarioEditorLabel.doAfterTextChanged { dispatch(ScenarioEditorIntent.SetLabel(it?.toString().orEmpty())) }
        scenarioEditorDescription.doAfterTextChanged { dispatch(ScenarioEditorIntent.SetDescription(it?.toString().orEmpty())) }
        enabled = bindSwitch(scenarioEditorEnabled, ScenarioEditorIntent::SetEnabled)
        continueOnError = bindSwitch(scenarioEditorContinueOnError, ScenarioEditorIntent::SetContinueOnError)
        scenarioEditorAddTrigger.setOnClickListener { showAddTriggerSheet() }
        scenarioEditorAddCondition.setOnClickListener { showAddConditionSheet() }
        scenarioEditorAddStep.setOnClickListener { showAddStepSheet() }
        scenarioEditorSave.setOnClickListener { dispatch(ScenarioEditorIntent.Save) }
        scenarioEditorDelete.setOnClickListener { dispatch(ScenarioEditorIntent.RequestDelete) }
        scenarioEditorCatalogStatus.setOnClickListener { dispatch(ScenarioEditorIntent.ReloadCatalog) }
    }

    override fun render(state: ScenarioEditorUiState) = with(binding) {
        val draft = state.draft
        scenarioEditorLabel.showText(draft.label)
        scenarioEditorDescription.showText(draft.description)
        enabled.render(draft.enabled)
        continueOnError.render(draft.continueOnError)
        scenarioEditorUnsupported.setVisible(state.hasUnsupportedParts)
        scenarioEditorAddTrigger.isEnabled = state.canAddTrigger
        scenarioEditorAddCondition.isEnabled = state.canAddCondition
        scenarioEditorAddStep.isEnabled = state.canAddStep
        scenarioEditorSave.isEnabled = state.canSave
        scenarioEditorDelete.isEnabled = state.canDelete
        scenarioEditorDelete.setVisible(state.isExisting)
        scenarioEditorProgress.setVisible(state.saving || state.loading)
        scenarioEditorError.setVisible(state.visibleError != null)
        scenarioEditorError.text = state.visibleError?.userMessage(requireContext())
        scenarioEditorCatalogStatus.setVisible(state.catalog.error != null)
        scenarioEditorCatalogStatus.text = state.catalog.error?.let { getString(R.string.scenarios_catalog_failed, it.userMessage(requireContext())) }
        sections.render(draft, state.catalog)
        renderPrompt(state.prompt, state.catalog)
        renderEditor(state.editor)
    }

    override fun onEvent(event: ScenarioEditorEvent) {
        when (event) {
            is ScenarioEditorEvent.Finished -> showMessage(event.message)
            is ScenarioEditorEvent.OpenRoute -> navigate(event.route)
            is ScenarioEditorEvent.Error -> showError(event.message)
        }
    }

    override fun onBindingDestroyed() = promptSlot.clear()

    private fun renderEditor(form: EditorForm?) {
        form ?: return
        val tag = ScenarioFormDialog.tagOf(form)
        if (childFragmentManager.findFragmentByTag(tag) != null) return
        dialogFor(form).show(childFragmentManager, tag)
    }

    private fun dialogFor(form: EditorForm): DialogFragment = when (form) {
        is SpeakForm -> SpeakFormDialog()
        is DelayForm -> DelayFormDialog()
        is EntityConditionForm -> EntityConditionFormDialog()
        is TimeTriggerForm -> TimeTriggerFormDialog()
        is TimeWindowForm -> TimeWindowFormDialog()
        is LightForm -> LightFormDialog()
    }

    private fun renderPrompt(prompt: ScenarioPrompt?, catalog: CatalogState) {
        val dismiss = { dispatch(ScenarioEditorIntent.DismissPrompt) }
        when {
            prompt == null -> promptSlot.clear()
            prompt is ScenarioPrompt.PickEntry && catalog.loading -> promptSlot.show(CATALOG_WAIT, dismiss, ::catalogWait)
            prompt is ScenarioPrompt.PickEntry && catalog.error != null -> promptSlot.show(catalog.error, dismiss) { catalogError(catalog.error) }
            else -> promptSlot.show(prompt to catalog.entries, dismiss) { promptDialog(prompt, catalog) }
        }
    }

    private fun promptDialog(prompt: ScenarioPrompt, catalog: CatalogState) = when (prompt) {
        is ScenarioPrompt.PickEntry -> catalogPicker(prompt, catalog)
        is ScenarioPrompt.ChooseAction -> actionSheet(prompt)
        ScenarioPrompt.ConfirmDelete -> confirmDelete()
        ScenarioPrompt.Conflict -> conflict()
    }

    private fun catalogPicker(prompt: ScenarioPrompt.PickEntry, catalog: CatalogState) = CatalogPickerDialog(
        requireContext(),
        catalog.entries.sortedByDescending { it.id == prompt.initialId },
        prompt.includeSettings,
        onSelect = { dispatch(ScenarioEditorIntent.EntryPicked(it)) },
        onShowSettings = { dispatch(ScenarioEditorIntent.ShowSettingsEntries(it)) },
    ).dialog

    private fun actionSheet(prompt: ScenarioPrompt.ChooseAction) = ChoiceSheet(
        requireContext(),
        R.string.scenarios_action_dialog_title,
        prompt.actions.map { wire -> choice(ScenarioFormat.actionLabel(requireContext(), wire), actionIcon(wire), ScenarioEditorIntent.ActionChosen(wire)) },
    ).dialog

    private fun actionIcon(wire: String): Int =
        if (wire == ScenarioPromptFlow.ACTION_SPEAK) R.drawable.scenarios_ic_speak else R.drawable.scenarios_ic_tune

    private fun catalogWait() = MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.scenarios_catalog_loading)
        .setView(ProgressBar(requireContext()).apply { isIndeterminate = true })
        .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun catalogError(failure: RemoteFailure) = MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.scenarios_catalog_error_title)
        .setMessage(failure.userMessage(requireContext()))
        .setPositiveButton(R.string.scenarios_retry) { _, _ -> dispatch(ScenarioEditorIntent.ReloadCatalog) }
        .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun confirmDelete() = MaterialAlertDialogBuilder(requireContext())
        .setMessage(R.string.scenarios_delete_confirm)
        .setPositiveButton(R.string.scenarios_delete_yes) { _, _ -> dispatch(ScenarioEditorIntent.ConfirmDelete) }
        .setNegativeButton(R.string.scenarios_cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun conflict() = MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.scenarios_conflict_title)
        .setMessage(R.string.scenarios_conflict_message)
        .setPositiveButton(R.string.scenarios_conflict_reload) { _, _ -> dispatch(ScenarioEditorIntent.Reload) }
        .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun showAddTriggerSheet() = showMenu(
        R.string.scenarios_editor_add_trigger_title,
        choice(R.string.scenarios_editor_add_trigger_time, R.drawable.scenarios_ic_schedule, ScenarioEditorIntent.AddTimeTrigger),
    )

    private fun showAddConditionSheet() = showMenu(
        R.string.scenarios_editor_add_condition_title,
        choice(R.string.scenarios_editor_add_condition_entity, R.drawable.scenarios_ic_devices, ScenarioEditorIntent.AddEntityCondition),
        choice(R.string.scenarios_editor_add_condition_window, R.drawable.scenarios_ic_schedule, ScenarioEditorIntent.AddTimeWindow),
    )

    private fun showAddStepSheet() = showMenu(
        R.string.scenarios_editor_add_step_title,
        choice(R.string.scenarios_editor_add_action, R.drawable.scenarios_ic_tune, ScenarioEditorIntent.AddCatalogStep),
        choice(R.string.scenarios_editor_add_speak, R.drawable.scenarios_ic_speak, ScenarioEditorIntent.AddSpeakStep),
        choice(R.string.scenarios_editor_add_delay, R.drawable.scenarios_ic_timer, ScenarioEditorIntent.AddDelayStep),
    )

    private fun showMenu(@StringRes title: Int, vararg choices: ChoiceSheet.Choice) =
        ChoiceSheet(requireContext(), title, choices.toList()).dialog.show()

    private fun choice(@StringRes label: Int, @DrawableRes icon: Int, intent: ScenarioEditorIntent) =
        choice(getString(label), icon, intent)

    private fun choice(label: String, @DrawableRes icon: Int, intent: ScenarioEditorIntent) =
        ChoiceSheet.Choice(label, icon) { dispatch(intent) }

    private companion object {
        const val CATALOG_WAIT = "catalog_wait"
    }
}

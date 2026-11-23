package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.presentation.bindSwitch
import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import android.widget.ProgressBar
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.DialogFragment
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.presentation.showNotice
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenarioEditorBinding
import dev.proflyder.home.feature.scenarios.presentation.CatalogPickerDialog
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
    private val menus = ScenarioEditorMenus(this)

    override fun onScreenCreate(savedInstanceState: Bundle?) {
        onResult(EditorMenuChosen, ::menuIntent)
        onResult(EditorActionChosen, ::actionIntent)
    }

    override fun onViewReady(binding: FragmentScenarioEditorBinding, savedInstanceState: Bundle?) = with(binding) {
        root.applyScrollBottomInset()
        root.keepFocusedVisible()
        val sections = ScenarioDraftSections(binding, ::dispatch)
        scenarioEditorLabel.doAfterTextChanged { dispatch(ScenarioEditorIntent.SetLabel(it?.toString().orEmpty())) }
        scenarioEditorDescription.doAfterTextChanged { dispatch(ScenarioEditorIntent.SetDescription(it?.toString().orEmpty())) }
        val enabled = bindSwitch(scenarioEditorEnabled, ScenarioEditorIntent::SetEnabled)
        val continueOnError = bindSwitch(scenarioEditorContinueOnError, ScenarioEditorIntent::SetContinueOnError)
        scenarioEditorAddTrigger.setOnClickListener { menus.open(EditorMenu.TRIGGER) }
        scenarioEditorAddCondition.setOnClickListener { menus.open(EditorMenu.CONDITION) }
        scenarioEditorAddStep.setOnClickListener { menus.open(EditorMenu.STEP) }
        scenarioEditorSave.setOnClickListener { dispatch(ScenarioEditorIntent.Save) }
        scenarioEditorDelete.setOnClickListener { dispatch(ScenarioEditorIntent.RequestDelete) }
        scenarioEditorCatalogStatus.setOnClickListener { dispatch(ScenarioEditorIntent.ReloadCatalog) }
        render({ it.draft.label }, scenarioEditorLabel::showText)
        render({ it.draft.description }, scenarioEditorDescription::showText)
        render({ it.draft.enabled }, enabled::render)
        render({ it.draft.continueOnError }, continueOnError::render)
        render(::editorControlsOf, ::renderControls)
        render(ScenarioEditorUiState::visibleError, ::renderError)
        render({ it.catalog.error }, ::renderCatalogStatus)
        render({ it.draft to it.catalog }) { (draft, catalog) -> sections.render(draft, catalog) }
        render({ it.prompt to it.catalog }) { (prompt, catalog) -> renderPrompt(prompt, catalog) }
        render(ScenarioEditorUiState::editor, ::renderEditor)
    }

    private fun renderControls(controls: EditorControls) = with(binding) {
        scenarioEditorUnsupported.setVisible(controls.hasUnsupportedParts)
        scenarioEditorAddTrigger.isEnabled = controls.canAddTrigger
        scenarioEditorAddCondition.isEnabled = controls.canAddCondition
        scenarioEditorAddStep.isEnabled = controls.canAddStep
        scenarioEditorSave.isEnabled = controls.canSave
        scenarioEditorDelete.isEnabled = controls.canDelete
        scenarioEditorDelete.setVisible(controls.isExisting)
        scenarioEditorProgress.setVisible(controls.busy)
    }

    private fun renderError(error: RemoteFailure?) {
        binding.scenarioEditorError.setVisible(error != null)
        binding.scenarioEditorError.text = error?.userMessage(requireContext())
    }

    private fun renderCatalogStatus(error: RemoteFailure?) {
        binding.scenarioEditorCatalogStatus.setVisible(error != null)
        binding.scenarioEditorCatalogStatus.text = error?.let { getString(R.string.scenarios_catalog_failed, it.userMessage(requireContext())) }
    }

    override fun onEvent(event: ScenarioEditorEvent) {
        when (event) {
            is ScenarioEditorEvent.Finished -> showNotice(event.notice)
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
        menus.renderActions(prompt as? ScenarioPrompt.ChooseAction)
        when {
            prompt == null || prompt is ScenarioPrompt.ChooseAction -> promptSlot.clear()
            prompt is ScenarioPrompt.PickEntry && catalog.loading -> promptSlot.show(CATALOG_WAIT, dismiss, ::catalogWait)
            prompt is ScenarioPrompt.PickEntry && catalog.error != null -> promptSlot.show(catalog.error, dismiss) { catalogError(catalog.error) }
            else -> promptSlot.show(prompt to catalog.entries, dismiss) { promptDialog(prompt, catalog) }
        }
    }

    private fun promptDialog(prompt: ScenarioPrompt, catalog: CatalogState) = when (prompt) {
        is ScenarioPrompt.PickEntry -> catalogPicker(prompt, catalog)
        is ScenarioPrompt.ChooseAction -> error("action choice is rendered by ScenarioEditorMenus")
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

    private fun catalogWait() = MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.scenarios_catalog_loading)
        .setView(ProgressBar(requireContext()).apply { isIndeterminate = true })
        .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun catalogError(failure: RemoteFailure) = MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.scenarios_catalog_error_title)
        .setMessage(failure.userMessage(requireContext()))
        .setPositiveButton(DesignR.string.ds_action_retry) { _, _ -> dispatch(ScenarioEditorIntent.ReloadCatalog) }
        .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun confirmDelete() = MaterialAlertDialogBuilder(requireContext())
        .setMessage(R.string.scenarios_delete_confirm)
        .setPositiveButton(DesignR.string.ds_action_delete) { _, _ -> dispatch(ScenarioEditorIntent.ConfirmDelete) }
        .setNegativeButton(DesignR.string.ds_action_cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private fun conflict() = MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.scenarios_conflict_title)
        .setMessage(R.string.scenarios_conflict_message)
        .setPositiveButton(R.string.scenarios_conflict_reload) { _, _ -> dispatch(ScenarioEditorIntent.Reload) }
        .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(ScenarioEditorIntent.DismissPrompt) }
        .create()

    private companion object {
        const val CATALOG_WAIT = "catalog_wait"
    }
}

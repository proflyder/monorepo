package dev.proflyder.home.feature.scenarios.presentation.editor

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.presentation.AppEventBus
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.scenarios.ScenarioValidation
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenarioCatalog
import dev.proflyder.home.feature.scenarios.domain.SaveScenario
import dev.proflyder.home.feature.scenarios.domain.ScenarioSaveSession
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.isScenarioConflict
import dev.proflyder.home.feature.scenarios.presentation.ScenariosChanged
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal class ScenarioEditorViewModel(
    private val loadScenario: LoadScenario,
    private val loadCatalog: LoadScenarioCatalog,
    private val deleteScenario: DeleteScenario,
    private val appEvents: AppEventBus,
    private val saved: SavedEditorState,
    private val session: ScenarioSaveSession,
) : StateViewModel<ScenarioEditorUiState, ScenarioEditorIntent, ScenarioEditorEvent>(ScenarioEditorUiState()),
    EditorStateHost {
    private val prompts = ScenarioPromptFlow(this)
    private val loads by latest()

    override val current: ScenarioEditorUiState get() = currentState

    init {
        refreshCatalog()
        val restored = saved.draft
        val scenarioId = saved.scenarioId
        when {
            restored != null -> restore(restored)
            scenarioId != null -> load(scenarioId)
            else -> saved.draft = currentState.draft
        }
    }

    override fun update(reducer: ScenarioEditorUiState.() -> ScenarioEditorUiState) = updateState(reducer)

    override fun notify(event: ScenarioEditorEvent) = emit(event)

    override fun editDraft(transform: (ScenarioEditorDraft) -> ScenarioEditorDraft) {
        session.invalidate()
        updateState { copy(draft = transform(draft), error = null, prompt = prompt.takeUnless { it == ScenarioPrompt.Conflict }) }
        saved.draft = currentState.draft
    }

    override fun handle(intent: ScenarioEditorIntent) {
        when (intent) {
            is ScenarioEditorIntent.SetLabel -> editDraft { it.copy(label = intent.value) }
            is ScenarioEditorIntent.SetDescription -> editDraft { it.copy(description = intent.value) }
            is ScenarioEditorIntent.SetEnabled -> editDraft { it.copy(enabled = intent.value) }
            is ScenarioEditorIntent.SetContinueOnError -> editDraft { it.copy(continueOnError = intent.value) }
            is ScenarioEditorIntent.Remove -> editDraft { it.removed(intent.section, intent.index) }
            is ScenarioEditorIntent.MoveStep -> editDraft { it.copy(steps = it.steps.moved(intent.index, intent.delta)) }
            else -> handlePrompt(intent)
        }
    }

    private fun handlePrompt(intent: ScenarioEditorIntent) {
        when (intent) {
            ScenarioEditorIntent.AddTimeTrigger -> prompts.addTimeTrigger()
            ScenarioEditorIntent.AddTimeWindow -> prompts.addTimeWindow()
            ScenarioEditorIntent.AddEntityCondition -> pickEntry(EntryPurpose.NewCondition)
            ScenarioEditorIntent.AddCatalogStep -> pickEntry(EntryPurpose.Step(EditTarget.New))
            ScenarioEditorIntent.AddSpeakStep -> prompts.open(SpeakForm(EditTarget.New, ""))
            ScenarioEditorIntent.AddDelayStep -> prompts.open(DelayForm.of(EditTarget.New, DelayForm.DEFAULT_SECONDS))
            ScenarioEditorIntent.PickConditionEntity -> pickEntry(EntryPurpose.ConditionEntity)
            is ScenarioEditorIntent.Edit -> prompts.edit(intent.section, intent.index)
            is ScenarioEditorIntent.EntryPicked -> prompts.entryPicked(intent.entryId)
            is ScenarioEditorIntent.ShowSettingsEntries -> prompts.showSettingsEntries(intent.visible)
            is ScenarioEditorIntent.ActionChosen -> prompts.actionChosen(intent.action)
            is ScenarioEditorIntent.UpdateForm -> prompts.updateForm(intent.form)
            ScenarioEditorIntent.CommitForm -> prompts.commitForm()
            ScenarioEditorIntent.CancelForm -> updateState { copy(editor = null) }
            ScenarioEditorIntent.DismissPrompt -> prompts.prompt(null)
            else -> handleCommand(intent)
        }
    }

    private fun handleCommand(intent: ScenarioEditorIntent) {
        when (intent) {
            ScenarioEditorIntent.ReloadCatalog -> refreshCatalog()
            ScenarioEditorIntent.Save -> save()
            ScenarioEditorIntent.RequestDelete -> prompts.prompt(ScenarioPrompt.ConfirmDelete)
            ScenarioEditorIntent.ConfirmDelete -> delete()
            ScenarioEditorIntent.Reload -> reload()
            else -> Unit
        }
    }

    private fun pickEntry(purpose: EntryPurpose) {
        if (currentState.catalog.error != null) refreshCatalog()
        prompts.prompt(ScenarioPrompt.PickEntry(purpose))
        closeEmptyPicker()
    }

    private fun closeEmptyPicker() {
        val state = currentState
        if (state.prompt !is ScenarioPrompt.PickEntry || !state.catalog.loaded || state.catalog.entries.isNotEmpty()) return
        updateState { copy(prompt = null) }
        emit(ScenarioEditorEvent.Error(R.string.scenarios_catalog_empty))
    }

    private fun restore(draft: ScenarioEditorDraft) {
        updateState { copy(existing = draft.restoredExisting(), draft = draft) }
        if (draft.existingId == null) session.restore(draft.toScenarioDraft())
    }

    private fun refreshCatalog() {
        if (currentState.catalog.loading) return
        updateState { copy(catalog = catalog.copy(loading = true, error = null)) }
        launch {
            val result = loadCatalog()
            updateState { copy(catalog = catalog.loadedFrom(result)) }
            closeEmptyPicker()
        }
    }

    private fun load(id: String) {
        updateState { copy(loading = true, error = null, prompt = null) }
        loads.launch { onLoaded(loadScenario(id)) }
    }

    private fun reload() {
        val id = currentState.existing?.id ?: currentState.draft.existingId ?: return
        load(id)
    }

    private fun onLoaded(result: Outcome<Scenario>) {
        when (result) {
            is Outcome.Success -> adopt(result.value)
            is Outcome.Failure -> updateState { copy(loading = false, error = result.failure, loadFailed = existing == null) }
        }
    }

    private fun adopt(scenario: Scenario) {
        updateState {
            copy(loading = false, saving = false, existing = scenario, draft = ScenarioEditorDraftCodec.fromScenario(scenario), error = null, loadFailed = false)
        }
        saved.draft = currentState.draft
    }

    private fun save() {
        val state = currentState
        if (state.saving) return
        val payload = state.draft.toScenarioDraft()
        val problem = if (state.hasUnsupportedParts) RemoteFailure.Validation(Field.UNSUPPORTED) else ScenarioValidation.validate(payload)
        if (problem != null) return updateState { copy(error = problem) }
        updateState { copy(saving = true, error = null, prompt = null) }
        launch { onSaved(session.save(state.existing ?: state.draft.restoredExisting(), payload)) }
    }

    private fun onSaved(result: Outcome<Scenario>) {
        when (result) {
            is Outcome.Success -> adopt(result.value).also { finish(R.string.scenarios_saved) }
            is Outcome.Failure -> updateState {
                copy(saving = false, error = result.failure, prompt = ScenarioPrompt.Conflict.takeIf { result.failure.isScenarioConflict() })
            }
        }
    }

    private fun delete() {
        val existing = currentState.existing ?: return
        if (currentState.saving) return
        updateState { copy(saving = true, error = null, prompt = null) }
        launch {
            when (val result = deleteScenario(existing.id)) {
                is Outcome.Success -> finish(R.string.scenarios_deleted).also { updateState { copy(saving = false) } }
                is Outcome.Failure -> updateState { copy(saving = false, error = result.failure) }
            }
        }
    }

    private fun finish(message: Int) {
        appEvents.publish(ScenariosChanged)
        emit(ScenarioEditorEvent.Finished(message))
    }
}

internal fun CatalogState.loadedFrom(result: Outcome<ControlCatalog>): CatalogState = when (result) {
    is Outcome.Success -> CatalogState(entries = result.value.entries)
    is Outcome.Failure -> copy(loading = false, error = result.failure)
}

@ContributesIntoSet(AppScope::class)
@Inject
class ScenarioEditorViewModelFactoryContributor(
    private val loadScenario: LoadScenario,
    private val loadCatalog: LoadScenarioCatalog,
    private val saveScenario: SaveScenario,
    private val deleteScenario: DeleteScenario,
    private val settings: ConnectionSettingsStore,
    private val events: AppEventBus,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ScenarioEditorViewModel::class.java

    override fun createFactory(): ViewModelProvider.Factory = SavedStateViewModelFactory { handle -> create(handle) }

    private fun create(handle: SavedStateHandle): ScenarioEditorViewModel {
        val saved = SavedEditorState(handle)
        val session = ScenarioSaveSession(saveScenario, settings, saved)
        return ScenarioEditorViewModel(loadScenario, loadCatalog, deleteScenario, events, saved, session)
    }
}

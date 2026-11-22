package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.navigation.Route
import androidx.annotation.StringRes
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioLimits

internal data class ScenarioEditorUiState(
    val loading: Boolean = false,
    val saving: Boolean = false,
    val existing: Scenario? = null,
    val draft: ScenarioEditorDraft = ScenarioEditorDraft(),
    val catalog: CatalogState = CatalogState(),
    val error: RemoteFailure? = null,
    val loadFailed: Boolean = false,
    val prompt: ScenarioPrompt? = null,
    val editor: EditorForm? = null,
) {
    val hasUnsupportedParts: Boolean get() = draft.hasUnsupportedParts
    val isExisting: Boolean get() = existing != null || draft.existingId != null
    val canSave: Boolean get() = !saving && !loading && !loadFailed && !hasUnsupportedParts
    val canDelete: Boolean get() = !saving && !loading && !loadFailed
    val canAddTrigger: Boolean get() = draft.triggers.size < ScenarioLimits.MAX_TRIGGERS
    val canAddCondition: Boolean get() = draft.conditions.size < ScenarioLimits.MAX_CONDITIONS
    val canAddStep: Boolean get() = draft.steps.size < ScenarioLimits.MAX_STEPS
    val visibleError: RemoteFailure? get() = error.takeIf { prompt != ScenarioPrompt.Conflict }
}

internal sealed interface ScenarioEditorIntent {
    data class SetLabel(val value: String) : ScenarioEditorIntent
    data class SetDescription(val value: String) : ScenarioEditorIntent
    data class SetEnabled(val value: Boolean) : ScenarioEditorIntent
    data class SetContinueOnError(val value: Boolean) : ScenarioEditorIntent
    data object AddTimeTrigger : ScenarioEditorIntent
    data object AddEntityCondition : ScenarioEditorIntent
    data object AddTimeWindow : ScenarioEditorIntent
    data object AddCatalogStep : ScenarioEditorIntent
    data object AddSpeakStep : ScenarioEditorIntent
    data object AddDelayStep : ScenarioEditorIntent
    data class Edit(val section: DraftSection, val index: Int) : ScenarioEditorIntent
    data class Remove(val section: DraftSection, val index: Int) : ScenarioEditorIntent
    data class MoveStep(val index: Int, val delta: Int) : ScenarioEditorIntent
    data object PickConditionEntity : ScenarioEditorIntent
    data class EntryPicked(val entryId: String) : ScenarioEditorIntent
    data class ShowSettingsEntries(val visible: Boolean) : ScenarioEditorIntent
    data class ActionChosen(val action: String) : ScenarioEditorIntent
    data object DismissPrompt : ScenarioEditorIntent
    data class UpdateForm(val form: EditorForm) : ScenarioEditorIntent
    data object CommitForm : ScenarioEditorIntent
    data object CancelForm : ScenarioEditorIntent
    data object ReloadCatalog : ScenarioEditorIntent
    data object Save : ScenarioEditorIntent
    data object RequestDelete : ScenarioEditorIntent
    data object ConfirmDelete : ScenarioEditorIntent
    data object Reload : ScenarioEditorIntent
}

internal sealed interface ScenarioEditorEvent {
    data class Finished(@StringRes val message: Int) : ScenarioEditorEvent
    data class OpenRoute(val route: Route) : ScenarioEditorEvent
    data class Error(@StringRes val message: Int) : ScenarioEditorEvent
}

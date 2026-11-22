package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.RemoteFailure

internal sealed interface EntryPurpose {
    data class Step(val target: EditTarget, val initialAction: String? = null, val paramsJson: String = "{}") : EntryPurpose

    data object NewCondition : EntryPurpose

    data object ConditionEntity : EntryPurpose
}

internal sealed interface ScenarioPrompt {
    data class PickEntry(
        val purpose: EntryPurpose,
        val initialId: String? = null,
        val includeSettings: Boolean = false,
    ) : ScenarioPrompt

    data class ChooseAction(
        val entry: ControlEntry,
        val actions: List<String>,
        val purpose: EntryPurpose.Step,
    ) : ScenarioPrompt

    data object ConfirmDelete : ScenarioPrompt

    data object Conflict : ScenarioPrompt
}

internal data class CatalogState(
    val entries: List<ControlEntry> = emptyList(),
    val loading: Boolean = false,
    val error: RemoteFailure? = null,
) {
    val loaded: Boolean get() = !loading && error == null
}

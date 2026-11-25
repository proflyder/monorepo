package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.backgroundFailures
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.DeliveredWhenStarted
import dev.proflyder.home.core.presentation.ExclusiveActions
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.settle
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.di.AlarmsFeatureScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class AlarmSelectionUi(
    val active: Boolean = false,
    val count: Int = 0,
    val allSelected: Boolean = false,
    val actions: AlarmSelectionActions = AlarmSelectionActions(canEnable = false, canDisable = false),
    val busy: Boolean = false,
)

internal data class AlarmsListUi(
    val load: ListLoadState = ListLoadState(),
    val hasItems: Boolean = false,
)

internal data class AlarmsUiState(
    val load: ListLoadState = ListLoadState(),
    val alarms: List<Schedule> = emptyList(),
    val busy: Boolean = false,
    val selectedIds: Set<String> = emptySet(),
    val list: AlarmsListUi = AlarmsListUi(),
    val rows: List<AlarmRow> = emptyList(),
    val selection: AlarmSelectionUi = AlarmSelectionUi(),
)

internal fun AlarmsUiState.derivedFrom(previous: AlarmsUiState): AlarmsUiState {
    val sourcesChanged = alarms != previous.alarms || selectedIds != previous.selectedIds
    if (!sourcesChanged && load == previous.load && busy == previous.busy) {
        return copy(list = previous.list, rows = previous.rows, selection = previous.selection)
    }
    val active = selectedIds.isNotEmpty()
    return copy(
        list = AlarmsListUi(load, alarms.isNotEmpty()),
        rows = if (sourcesChanged) alarms.map { AlarmRow(it, it.id in selectedIds, active) } else previous.rows,
        selection = AlarmSelectionUi(
            active = active,
            count = selectedIds.size,
            allSelected = alarms.isNotEmpty() && selectedIds.size == alarms.size,
            actions = if (sourcesChanged) alarmSelectionActions(alarms, selectedIds) else previous.selection.actions,
            busy = busy,
        ),
    )
}

internal sealed interface AlarmsIntent {
    data class Refresh(val mode: RefreshMode) : AlarmsIntent
    data object Add : AlarmsIntent
    data object OpenConnection : AlarmsIntent
    data class Click(val id: String) : AlarmsIntent
    data class LongClick(val id: String) : AlarmsIntent
    data object ToggleSelectAll : AlarmsIntent
    data object ClearSelection : AlarmsIntent
    data object RequestDeleteSelected : AlarmsIntent
    data class SetSelectedEnabled(val enabled: Boolean) : AlarmsIntent
    data object DeleteSelected : AlarmsIntent
}

internal sealed interface AlarmsEvent {
    data class Failure(val failure: RemoteFailure) : AlarmsEvent, DeliveredWhenStarted
    data class OpenRoute(val route: Route) : AlarmsEvent
    data class ConfirmDeleteSelected(val count: Int) : AlarmsEvent
}

internal class AlarmsViewModel(
    private val repository: AlarmRepository,
    wallMillis: () -> Long = System::currentTimeMillis,
) : StateViewModel<AlarmsUiState, AlarmsIntent, AlarmsEvent>(AlarmsUiState()) {
    val minute: StateFlow<Long> = minuteTicks(wallMillis)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(), wallMillis() / MILLIS_PER_MINUTE)

    private val bulk = ExclusiveActions(viewModelScope, ::onBusy) { emit(AlarmsEvent.Failure(it)) }

    init {
        combine(repository.items, repository.load, ::onStore).launchIn(viewModelScope)
        repository.backgroundFailures().onEach { emit(AlarmsEvent.Failure(it)) }.launchIn(viewModelScope)
    }

    override fun derive(previous: AlarmsUiState, next: AlarmsUiState): AlarmsUiState = next.derivedFrom(previous)

    override fun handle(intent: AlarmsIntent) {
        when (intent) {
            is AlarmsIntent.Refresh -> repository.refresh(intent.mode)
            AlarmsIntent.Add -> openEditor(null)
            AlarmsIntent.OpenConnection -> emit(AlarmsEvent.OpenRoute(Route.Tab(CONNECTION_FEATURE)))
            is AlarmsIntent.Click -> click(intent.id)
            is AlarmsIntent.LongClick -> toggleSelection(intent.id)
            AlarmsIntent.ToggleSelectAll -> updateState { copy(selectedIds = if (selection.allSelected) emptySet() else ids()) }
            AlarmsIntent.ClearSelection -> updateState { copy(selectedIds = emptySet()) }
            is AlarmsIntent.SetSelectedEnabled -> setSelectedEnabled(intent.enabled)
            AlarmsIntent.RequestDeleteSelected -> confirmDeleteSelected()
            AlarmsIntent.DeleteSelected -> deleteSelected()
        }
    }

    private fun onStore(alarms: List<Schedule>, load: ListLoadState) = updateState {
        copy(load = load, alarms = alarms, selectedIds = selectedIds intersect alarms.ids())
    }

    private fun openEditor(id: String?) {
        emit(AlarmsEvent.OpenRoute(Route.Screen(AlarmEditorDestination(id))))
    }

    private fun onBusy(key: String?) = updateState { copy(busy = key != null) }

    private fun click(id: String) = when (currentState.selection.active) {
        true -> toggleSelection(id)
        false -> openEditor(id)
    }

    private fun toggleSelection(id: String) = updateState {
        copy(selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id)
    }

    private fun setSelectedEnabled(enabled: Boolean) = bulk.runAll(BULK, selected()) { alarm ->
        repository.setEnabled(alarm.id, enabled).settle { updateState { copy(selectedIds = selectedIds - alarm.id) } }
    }

    private fun confirmDeleteSelected() {
        val count = currentState.selectedIds.size
        if (count > 0) emit(AlarmsEvent.ConfirmDeleteSelected(count))
    }

    private fun deleteSelected() = bulk.runAll(BULK, selected()) { alarm -> repository.delete(alarm.id).settle { } }

    private fun selected(): List<Schedule> = currentState.alarms.filter { it.id in currentState.selectedIds }

    private fun AlarmsUiState.ids(): Set<String> = alarms.ids()

    private fun List<Schedule>.ids(): Set<String> = mapTo(LinkedHashSet()) { it.id }

    private companion object {
        const val BULK = "selection"
        const val MILLIS_PER_MINUTE = 60_000L
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AlarmsViewModel::class)
@Inject
class AlarmsViewModelSource internal constructor(
    private val scope: AlarmsFeatureScope,
) : ViewModelSource {
    override fun create(): ViewModel = scope.viewModel { AlarmsViewModel(it.repository) }
}

private const val CONNECTION_FEATURE = "connection"

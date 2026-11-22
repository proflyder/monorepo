package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.core.navigation.Route
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.AppEventBus
import dev.proflyder.home.core.presentation.ExclusiveActions
import dev.proflyder.home.core.presentation.ListLoadState
import dev.proflyder.home.core.presentation.ListRefresher
import dev.proflyder.home.core.presentation.LoadStep
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.eventsOf
import dev.proflyder.home.core.presentation.settle
import dev.proflyder.home.core.presentation.valueOr
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.alarms.AlarmDisableChoice
import dev.proflyder.home.feature.alarms.AlarmOrder
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.alarmDisableChoice
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.merge

internal data class AlarmsUiState(
    val load: ListLoadState = ListLoadState(),
    val alarms: List<Schedule> = emptyList(),
    val busy: Boolean = false,
    val selectedIds: Set<String> = emptySet(),
) {
    val selectionMode: Boolean get() = selectedIds.isNotEmpty()
    val allSelected: Boolean get() = alarms.isNotEmpty() && selectedIds.size == alarms.size
    val rows: List<AlarmRow> get() = alarms.map { AlarmRow(it, it.id in selectedIds, selectionMode) }
    val selectionActions: AlarmSelectionActions get() = alarmSelectionActions(alarms, selectedIds)
}

internal sealed interface AlarmsIntent {
    data class Refresh(val mode: RefreshMode) : AlarmsIntent
    data object Add : AlarmsIntent
    data object OpenConnection : AlarmsIntent
    data class Click(val id: String) : AlarmsIntent
    data class LongClick(val id: String) : AlarmsIntent
    data class Toggle(val id: String, val enabled: Boolean) : AlarmsIntent
    data class SkipOnce(val id: String) : AlarmsIntent
    data class TurnOff(val id: String) : AlarmsIntent
    data class Snooze(val id: String) : AlarmsIntent
    data class Stop(val id: String) : AlarmsIntent
    data class RequestDelete(val id: String) : AlarmsIntent
    data class Delete(val id: String) : AlarmsIntent
    data object ToggleSelectAll : AlarmsIntent
    data object ClearSelection : AlarmsIntent
    data object RequestDeleteSelected : AlarmsIntent
    data class SetSelectedEnabled(val enabled: Boolean) : AlarmsIntent
    data object DeleteSelected : AlarmsIntent
}

internal sealed interface AlarmsEvent {
    data class Failure(val failure: RemoteFailure) : AlarmsEvent
    data class OpenRoute(val route: Route) : AlarmsEvent
    data class AskDisable(val id: String) : AlarmsEvent
    data class ConfirmDelete(val id: String) : AlarmsEvent
    data class ConfirmDeleteSelected(val count: Int) : AlarmsEvent
}

internal class AlarmsViewModel(
    private val repository: AlarmRepository,
    private val snoozeAlarm: SnoozeAlarm,
    bus: AppEventBus,
) : StateViewModel<AlarmsUiState, AlarmsIntent, AlarmsEvent>(AlarmsUiState()) {
    private val loader = ListRefresher(viewModelScope, repository::list, ::onLoad)
    private val actions = ExclusiveActions(viewModelScope, ::onBusy) { emit(AlarmsEvent.Failure(it)) }

    init {
        loader.refreshOn(merge(screenShown, bus.eventsOf<AlarmsChanged>()))
    }

    private fun openEditor(id: String?) {
        emit(AlarmsEvent.OpenRoute(Route.Screen(AlarmEditorDestination(id))))
    }

    override fun handle(intent: AlarmsIntent) {
        when (intent) {
            is AlarmsIntent.Refresh -> loader.refresh(intent.mode)
            AlarmsIntent.Add -> openEditor(null)
            AlarmsIntent.OpenConnection -> emit(AlarmsEvent.OpenRoute(Route.Tab(CONNECTION_FEATURE)))
            is AlarmsIntent.Click -> click(intent.id)
            is AlarmsIntent.LongClick -> toggleSelection(intent.id)
            is AlarmsIntent.Toggle -> toggle(intent.id, intent.enabled)
            is AlarmsIntent.SkipOnce -> replace(intent.id) { repository.skipNextOccurrence(it) }
            is AlarmsIntent.TurnOff -> replace(intent.id) { repository.setEnabled(it, false) }
            is AlarmsIntent.Snooze -> replace(intent.id) { snoozeAlarm(it) }
            is AlarmsIntent.Stop -> replace(intent.id) { repository.stop(it) }
            is AlarmsIntent.RequestDelete -> emit(AlarmsEvent.ConfirmDelete(intent.id))
            is AlarmsIntent.Delete -> delete(intent.id)
            else -> handleSelection(intent)
        }
    }

    private fun handleSelection(intent: AlarmsIntent) {
        when (intent) {
            AlarmsIntent.ToggleSelectAll -> updateState { copy(selectedIds = if (allSelected) emptySet() else ids()) }
            AlarmsIntent.ClearSelection -> updateState { copy(selectedIds = emptySet()) }
            is AlarmsIntent.SetSelectedEnabled -> setSelectedEnabled(intent.enabled)
            AlarmsIntent.RequestDeleteSelected -> confirmDeleteSelected()
            AlarmsIntent.DeleteSelected -> deleteSelected()
            else -> Unit
        }
    }

    private fun onLoad(step: LoadStep<List<Schedule>>) {
        updateState {
            val loaded = step.valueOr(alarms).sortedWith(AlarmOrder)
            copy(load = load.after(step), alarms = loaded, selectedIds = selectedIds intersect loaded.ids())
        }
        if (step is LoadStep.Failed && currentState.alarms.isNotEmpty()) emit(AlarmsEvent.Failure(step.failure))
    }

    private fun onBusy(key: String?) = updateState { copy(busy = key != null) }

    private fun click(id: String) = when (currentState.selectionMode) {
        true -> toggleSelection(id)
        false -> openEditor(id)
    }

    private fun toggleSelection(id: String) = updateState {
        copy(selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id)
    }

    private fun toggle(id: String, enabled: Boolean) {
        val alarm = currentState.alarms.firstOrNull { it.id == id } ?: return
        when {
            enabled -> replace(id) { repository.setEnabled(it, true) }
            alarmDisableChoice(alarm) == AlarmDisableChoice.DISABLE -> replace(id) { repository.setEnabled(it, false) }
            else -> emit(AlarmsEvent.AskDisable(id))
        }
    }

    private fun replace(id: String, action: suspend (String) -> Outcome<Schedule>) {
        actions.run(id, { action(id) }) { updated -> updateState { copy(alarms = alarms.withAlarm(updated)) } }
    }

    private fun delete(id: String) {
        actions.run(id, { repository.delete(id) }) { updateState { withoutAlarm(id) } }
    }

    private fun setSelectedEnabled(enabled: Boolean) = actions.runAll(BULK, selected()) { alarm ->
        repository.setEnabled(alarm.id, enabled).settle { updated ->
            updateState { copy(alarms = alarms.withAlarm(updated), selectedIds = selectedIds - alarm.id) }
        }
    }

    private fun confirmDeleteSelected() {
        val count = currentState.selectedIds.size
        if (count > 0) emit(AlarmsEvent.ConfirmDeleteSelected(count))
    }

    private fun deleteSelected() = actions.runAll(BULK, selected()) { alarm ->
        repository.delete(alarm.id).settle { updateState { withoutAlarm(alarm.id) } }
    }

    private fun selected(): List<Schedule> = currentState.alarms.filter { it.id in currentState.selectedIds }

    private fun AlarmsUiState.ids(): Set<String> = alarms.ids()

    private fun List<Schedule>.ids(): Set<String> = mapTo(LinkedHashSet()) { it.id }

    private fun List<Schedule>.withAlarm(updated: Schedule): List<Schedule> =
        map { if (it.id == updated.id) updated else it }

    private fun AlarmsUiState.withoutAlarm(id: String): AlarmsUiState =
        copy(alarms = alarms.filterNot { it.id == id }, selectedIds = selectedIds - id)

    private companion object {
        const val BULK = "selection"
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AlarmsViewModel::class)
@Inject
class AlarmsViewModelSource internal constructor(
    private val repository: AlarmRepository,
    private val snoozeAlarm: SnoozeAlarm,
    private val bus: AppEventBus,
) : ViewModelSource {
    override fun create(): ViewModel = AlarmsViewModel(repository, snoozeAlarm, bus)
}

private const val CONNECTION_FEATURE = "connection"

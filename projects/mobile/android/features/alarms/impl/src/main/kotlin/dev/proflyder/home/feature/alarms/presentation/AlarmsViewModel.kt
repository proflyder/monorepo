package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.alarms.domain.DeleteAlarm
import dev.proflyder.home.feature.alarms.domain.LoadAlarms
import dev.proflyder.home.feature.alarms.domain.SetAlarmEnabled
import dev.proflyder.home.feature.alarms.domain.SkipAlarmOccurrence
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import dev.proflyder.home.feature.alarms.domain.StopAlarm
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.Job

data class AlarmsUiState(
    val refreshing: Boolean = true,
    val alarms: List<Schedule> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val busy: Boolean = false,
    val selectedIds: Set<String> = emptySet(),
) {
    val selectionMode: Boolean get() = selectedIds.isNotEmpty()
    val allSelected: Boolean get() = alarms.isNotEmpty() && selectedIds.size == alarms.size
}

@Inject
internal class AlarmsViewModel(
    private val loadAlarms: LoadAlarms,
    private val setAlarmEnabled: SetAlarmEnabled,
    private val skipAlarmOccurrence: SkipAlarmOccurrence,
    private val deleteAlarm: DeleteAlarm,
    private val snoozeAlarm: SnoozeAlarm,
    private val stopAlarm: StopAlarm,
) : BaseViewModel<AlarmsUiState>(AlarmsUiState()) {
    private var refreshJob: Job? = null
    private var generation = 0

    fun refresh(silent: Boolean = false) {
        refreshJob?.cancel()
        val requested = ++generation
        updateState { copy(refreshing = refreshing || !silent, loadError = null) }
        refreshJob = launch {
            val result = loadAlarms()
            if (requested != generation) return@launch
            when (result) {
                is Outcome.Success -> updateState {
                    val sorted = result.value.sortedWith(compareBy({ it.at.hour }, { it.at.minute }, { it.label }))
                    copy(
                        refreshing = false,
                        alarms = sorted,
                        loadError = null,
                        selectedIds = selectedIds intersect sorted.map { it.id }.toSet(),
                    )
                }
                is Outcome.Failure -> updateState { copy(refreshing = false, loadError = result.failure) }
            }
        }
    }

    fun setEnabled(alarm: Schedule, enabled: Boolean) = mutate { setAlarmEnabled(alarm.id, enabled) }

    fun skipNextOccurrence(alarm: Schedule) = mutate { skipAlarmOccurrence(alarm.id) }

    fun snooze(alarm: Schedule) = mutate { snoozeAlarm(alarm.id) }

    fun stop(alarm: Schedule) = mutate { stopAlarm(alarm.id) }

    fun delete(alarm: Schedule) {
        if (currentState.busy) return
        updateState { copy(busy = true, actionError = null) }
        launch {
            when (val result = deleteAlarm(alarm.id)) {
                is Outcome.Success -> updateState { copy(busy = false, alarms = withoutAlarm(alarm.id)) }
                is Outcome.Failure -> updateState { copy(busy = false, actionError = result.failure) }
            }
        }
    }

    fun toggleSelection(id: String) = updateState {
        copy(selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id)
    }

    fun selectAll() = updateState { copy(selectedIds = alarms.mapTo(LinkedHashSet()) { it.id }) }

    fun clearSelection() = updateState { copy(selectedIds = emptySet()) }

    fun setSelectedEnabled(enabled: Boolean) = runBulk { alarm ->
        when (val result = setAlarmEnabled(alarm.id, enabled)) {
            is Outcome.Success -> {
                updateState { copy(alarms = withAlarm(result.value), selectedIds = selectedIds - alarm.id) }
                null
            }
            is Outcome.Failure -> result.failure
        }
    }

    fun deleteSelected() = runBulk { alarm ->
        when (val result = deleteAlarm(alarm.id)) {
            is Outcome.Success -> {
                updateState { copy(alarms = withoutAlarm(alarm.id), selectedIds = selectedIds - alarm.id) }
                null
            }
            is Outcome.Failure -> result.failure
        }
    }

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    private fun mutate(action: suspend () -> Outcome<Schedule>) {
        if (currentState.busy) return
        updateState { copy(busy = true, actionError = null) }
        launch {
            when (val result = action()) {
                is Outcome.Success -> updateState { copy(busy = false, alarms = withAlarm(result.value)) }
                is Outcome.Failure -> updateState { copy(busy = false, actionError = result.failure) }
            }
        }
    }

    private fun runBulk(step: suspend (Schedule) -> RemoteFailure?) {
        if (currentState.busy) return
        val targets = currentState.alarms.filter { it.id in currentState.selectedIds }
        if (targets.isEmpty()) return
        updateState { copy(busy = true, actionError = null) }
        launch {
            var failure: RemoteFailure? = null
            for (alarm in targets) {
                failure = step(alarm)
                if (failure != null) break
            }
            updateState { copy(busy = false, actionError = failure) }
        }
    }

    private fun AlarmsUiState.withAlarm(updated: Schedule): List<Schedule> =
        alarms.map { if (it.id == updated.id) updated else it }

    private fun AlarmsUiState.withoutAlarm(id: String): List<Schedule> = alarms.filterNot { it.id == id }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AlarmsViewModel::class)
@Inject
class AlarmsViewModelSource internal constructor(
    private val viewModel: () -> AlarmsViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

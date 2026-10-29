package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.alarms.domain.DeleteAlarm
import dev.proflyder.home.feature.alarms.domain.LoadAlarms
import dev.proflyder.home.feature.alarms.domain.SetAlarmEnabled
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import dev.proflyder.home.feature.alarms.domain.StopAlarm
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.Job

data class AlarmsUiState(
    val refreshing: Boolean = true,
    val alarms: List<Schedule> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    /** True while any single or bulk server mutation is in flight. */
    val busy: Boolean = false,
    /** Ids of alarms picked in multi-select mode; non-empty means selection mode. */
    val selectedIds: Set<String> = emptySet(),
) {
    val selectionMode: Boolean get() = selectedIds.isNotEmpty()
    val allSelected: Boolean get() = alarms.isNotEmpty() && selectedIds.size == alarms.size
}

class AlarmsViewModel(
    private val loadAlarms: LoadAlarms,
    private val setAlarmEnabled: SetAlarmEnabled,
    private val deleteAlarm: DeleteAlarm,
    private val snoozeAlarm: SnoozeAlarm,
    private val stopAlarm: StopAlarm,
) : BaseViewModel<AlarmsUiState>(AlarmsUiState()) {
    private var refreshJob: Job? = null
    private var generation = 0

    fun refresh() {
        refreshJob?.cancel()
        val requested = ++generation
        updateState { copy(refreshing = true, loadError = null) }
        refreshJob = launch {
            val result = loadAlarms()
            if (requested != generation) return@launch
            when (result) {
                is Outcome.Success -> updateState {
                    val sorted = result.value.sortedWith(compareBy({ it.at.hour }, { it.at.minute }, { it.label }))
                    // Drop selections of alarms that no longer exist on the server.
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

    // --- multi-select -----------------------------------------------------

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

    // --- one-shot error consumption ---------------------------------------

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    // --- internals --------------------------------------------------------

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

    /**
     * Applies [step] to each selected alarm in order. A step returns null on
     * success (and has already updated state) or the failure that stops the
     * run; alarms that were not processed stay selected so the user can retry.
     */
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

@ContributesIntoSet(AppScope::class)
@Inject
class AlarmsViewModelFactoryContributor(
    private val loadAlarms: LoadAlarms,
    private val setAlarmEnabled: SetAlarmEnabled,
    private val deleteAlarm: DeleteAlarm,
    private val snoozeAlarm: SnoozeAlarm,
    private val stopAlarm: StopAlarm,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = AlarmsViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        AlarmsViewModel(loadAlarms, setAlarmEnabled, deleteAlarm, snoozeAlarm, stopAlarm)
    }
}

package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class AlarmsUiState(
    val refreshing: Boolean = true,
    val alarms: List<Schedule> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val busyId: String? = null,
)

class AlarmsViewModel(
    private val loadAlarms: LoadAlarms,
    private val setAlarmEnabled: SetAlarmEnabled,
    private val deleteAlarm: DeleteAlarm,
    private val snoozeAlarm: SnoozeAlarm,
    private val stopAlarm: StopAlarm,
) : ViewModel() {
    private val mutableState = MutableStateFlow(AlarmsUiState())
    val state: StateFlow<AlarmsUiState> = mutableState.asStateFlow()

    private var refreshJob: Job? = null
    private var generation = 0

    fun refresh() {
        refreshJob?.cancel()
        val requested = ++generation
        mutableState.value = mutableState.value.copy(refreshing = true, loadError = null)
        refreshJob = viewModelScope.launch {
            val result = loadAlarms()
            if (requested != generation) return@launch
            when (result) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    refreshing = false,
                    alarms = result.value.sortedWith(compareBy({ it.at.hour }, { it.at.minute }, { it.label })),
                    loadError = null,
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    refreshing = false,
                    loadError = result.failure,
                )
            }
        }
    }

    fun setEnabled(alarm: Schedule, enabled: Boolean) = mutate(alarm.id) {
        setAlarmEnabled(alarm.id, enabled)
    }

    fun delete(alarm: Schedule) {
        if (mutableState.value.busyId != null) return
        mutableState.value = mutableState.value.copy(busyId = alarm.id, actionError = null)
        viewModelScope.launch {
            when (val result = deleteAlarm(alarm.id)) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    alarms = mutableState.value.alarms.filterNot { it.id == alarm.id },
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    actionError = result.failure,
                )
            }
        }
    }

    fun snooze(alarm: Schedule) = mutate(alarm.id) { snoozeAlarm(alarm.id) }

    fun stop(alarm: Schedule) = mutate(alarm.id) { stopAlarm(alarm.id) }

    fun consumeActionError() {
        mutableState.value = mutableState.value.copy(actionError = null)
    }

    fun consumeLoadError() {
        mutableState.value = mutableState.value.copy(loadError = null)
    }

    private fun mutate(id: String, action: suspend () -> Outcome<Schedule>) {
        if (mutableState.value.busyId != null) return
        mutableState.value = mutableState.value.copy(busyId = id, actionError = null)
        viewModelScope.launch {
            when (val result = action()) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    alarms = mutableState.value.alarms.map {
                        if (it.id == result.value.id) result.value else it
                    },
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    actionError = result.failure,
                )
            }
        }
    }
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

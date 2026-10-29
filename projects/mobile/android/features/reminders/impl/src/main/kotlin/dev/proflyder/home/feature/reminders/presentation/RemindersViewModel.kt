package dev.proflyder.home.feature.reminders.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.reminders.domain.DeleteReminder
import dev.proflyder.home.feature.reminders.domain.LoadReminders
import dev.proflyder.home.feature.reminders.domain.SetReminderEnabled
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

data class RemindersUiState(
    val refreshing: Boolean = true,
    val reminders: List<Schedule> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val busyId: String? = null,
)

class RemindersViewModel(
    private val loadReminders: LoadReminders,
    private val setEnabled: SetReminderEnabled,
    private val deleteReminder: DeleteReminder,
) : BaseViewModel<RemindersUiState>(RemindersUiState()) {

    private var refreshJob: Job? = null
    private var generation = 0

    fun refresh() {
        refreshJob?.cancel()
        val requested = ++generation
        mutableState.value = mutableState.value.copy(refreshing = true, loadError = null)
        refreshJob = viewModelScope.launch {
            val result = loadReminders()
            if (requested != generation) return@launch
            when (result) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    refreshing = false,
                    reminders = result.value.sortedBy { "${it.onceDate}${it.at.hour}${it.at.minute}" },
                    loadError = null,
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    refreshing = false,
                    loadError = result.failure,
                )
            }
        }
    }

    fun toggle(reminder: Schedule, enabled: Boolean) {
        if (mutableState.value.busyId != null) return
        mutableState.value = mutableState.value.copy(busyId = reminder.id, actionError = null)
        viewModelScope.launch {
            when (val result = setEnabled(reminder.id, enabled)) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    reminders = mutableState.value.reminders.map {
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

    fun delete(reminder: Schedule) {
        if (mutableState.value.busyId != null) return
        mutableState.value = mutableState.value.copy(busyId = reminder.id, actionError = null)
        viewModelScope.launch {
            when (val result = deleteReminder(reminder.id)) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    reminders = mutableState.value.reminders.filterNot { it.id == reminder.id },
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    actionError = result.failure,
                )
            }
        }
    }

    fun consumeActionError() {
        mutableState.value = mutableState.value.copy(actionError = null)
    }

    fun consumeLoadError() {
        mutableState.value = mutableState.value.copy(loadError = null)
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class RemindersViewModelFactoryContributor(
    private val loadReminders: LoadReminders,
    private val setEnabled: SetReminderEnabled,
    private val deleteReminder: DeleteReminder,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = RemindersViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        RemindersViewModel(loadReminders, setEnabled, deleteReminder)
    }
}

package dev.proflyder.home.feature.reminders.presentation

import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.core.navigation.Route
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
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
import dev.proflyder.home.core.presentation.valueOr
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.reminders.ReminderOrder
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.merge

internal data class RemindersUiState(
    val load: ListLoadState = ListLoadState(),
    val reminders: List<Schedule> = emptyList(),
    val busyId: String? = null,
)

internal sealed interface RemindersIntent {
    data class Refresh(val mode: RefreshMode) : RemindersIntent
    data object Add : RemindersIntent
    data class Open(val id: String) : RemindersIntent
    data class Toggle(val id: String, val enabled: Boolean) : RemindersIntent
    data class RequestDelete(val id: String) : RemindersIntent
    data class Delete(val id: String) : RemindersIntent
}

internal sealed interface RemindersEvent {
    data class Failure(val failure: RemoteFailure) : RemindersEvent
    data class OpenRoute(val route: Route) : RemindersEvent
    data class ConfirmDelete(val id: String) : RemindersEvent
}

internal class RemindersViewModel(
    private val repository: ReminderRepository,
    bus: AppEventBus,
) : StateViewModel<RemindersUiState, RemindersIntent, RemindersEvent>(RemindersUiState()) {
    private val loader = ListRefresher(viewModelScope, repository::list, ::onLoad)
    private val actions = ExclusiveActions(
        scope = viewModelScope,
        onBusy = { id -> updateState { copy(busyId = id) } },
        onFailure = { emit(RemindersEvent.Failure(it)) },
    )

    init {
        loader.refresh(RefreshMode.SOFT)
        loader.refreshOn(merge(visibility.returned, bus.eventsOf<RemindersChanged>()))
    }

    private fun openEditor(id: String?) {
        emit(RemindersEvent.OpenRoute(Route.Screen(ReminderEditorDestination(id))))
    }

    override fun handle(intent: RemindersIntent) {
        when (intent) {
            is RemindersIntent.Refresh -> loader.refresh(intent.mode)
            RemindersIntent.Add -> openEditor(null)
            is RemindersIntent.Open -> openEditor(intent.id)
            is RemindersIntent.Toggle -> toggle(intent.id, intent.enabled)
            is RemindersIntent.RequestDelete -> emit(RemindersEvent.ConfirmDelete(intent.id))
            is RemindersIntent.Delete -> delete(intent.id)
        }
    }

    private fun onLoad(step: LoadStep<List<Schedule>>) {
        updateState { copy(load = load.after(step), reminders = step.valueOr(reminders).sortedWith(ReminderOrder)) }
        if (step is LoadStep.Failed && currentState.reminders.isNotEmpty()) emit(RemindersEvent.Failure(step.failure))
    }

    private fun toggle(id: String, enabled: Boolean) {
        actions.run(id, { repository.setEnabled(id, enabled) }) { updated ->
            updateState { copy(reminders = reminders.map { if (it.id == updated.id) updated else it }) }
        }
    }

    private fun delete(id: String) {
        actions.run(id, { repository.delete(id) }) {
            updateState { copy(reminders = reminders.filterNot { it.id == id }) }
        }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(RemindersViewModel::class)
@Inject
class RemindersViewModelSource internal constructor(
    private val repository: ReminderRepository,
    private val bus: AppEventBus,
) : ViewModelSource {
    override fun create(): ViewModel = RemindersViewModel(repository, bus)
}

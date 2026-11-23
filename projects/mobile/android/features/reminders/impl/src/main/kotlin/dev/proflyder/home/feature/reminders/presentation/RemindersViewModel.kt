package dev.proflyder.home.feature.reminders.presentation

import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.core.navigation.Route
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.ExclusiveActions
import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.data.backgroundFailures
import dev.proflyder.home.feature.reminders.di.RemindersFeatureScope
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

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
) : StateViewModel<RemindersUiState, RemindersIntent, RemindersEvent>(RemindersUiState()) {
    private val actions = ExclusiveActions(
        scope = viewModelScope,
        onBusy = { id -> updateState { copy(busyId = id) } },
        onFailure = { emit(RemindersEvent.Failure(it)) },
    )

    init {
        combine(repository.items, repository.load, ::onStore).launchIn(viewModelScope)
        repository.backgroundFailures().onEach { emit(RemindersEvent.Failure(it)) }.launchIn(viewModelScope)
        repository.refreshOn(visibility.returned)
    }

    private fun openEditor(id: String?) {
        emit(RemindersEvent.OpenRoute(Route.Screen(ReminderEditorDestination(id))))
    }

    override fun handle(intent: RemindersIntent) {
        when (intent) {
            is RemindersIntent.Refresh -> repository.refresh(intent.mode)
            RemindersIntent.Add -> openEditor(null)
            is RemindersIntent.Open -> openEditor(intent.id)
            is RemindersIntent.Toggle -> actions.run(intent.id, { repository.setEnabled(intent.id, intent.enabled) }) { }
            is RemindersIntent.RequestDelete -> emit(RemindersEvent.ConfirmDelete(intent.id))
            is RemindersIntent.Delete -> actions.run(intent.id, { repository.delete(intent.id) }) { }
        }
    }

    private fun onStore(reminders: List<Schedule>, load: ListLoadState) = updateState {
        copy(load = load, reminders = reminders)
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(RemindersViewModel::class)
@Inject
class RemindersViewModelSource internal constructor(
    private val scope: RemindersFeatureScope,
) : ViewModelSource {
    override fun create(): ViewModel = scope.viewModel { RemindersViewModel(it.repository) }
}

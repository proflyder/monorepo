package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.ListItemIntent
import dev.proflyder.home.core.presentation.PagingHandler
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.UndoableRemoval
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope

internal data class NotificationHistoryUiState(
    val pages: PagedState<NotificationHistory> = PagedState(),
    val undo: NotificationHistory? = null,
)

internal sealed interface HistoryIntent {
    data class Open(val entry: NotificationHistory) : HistoryIntent
    data class ToggleRead(val key: String) : HistoryIntent
    data object ReadAll : HistoryIntent
    data object Clear : HistoryIntent
    data class Remove(val key: String) : HistoryIntent
}

internal sealed interface HistoryEvent {
    data object Cleared : HistoryEvent
    data class OpenRoute(val route: Route) : HistoryEvent
    data class Failure(val failure: RemoteFailure) : HistoryEvent
}

internal class NotificationHistoryViewModel(
    private val repository: NotificationHistoryRepository,
    appScope: CoroutineScope,
) : StateViewModel<NotificationHistoryUiState, HistoryIntent, HistoryEvent>(NotificationHistoryUiState()) {
    private val undo = UndoableRemoval(
        list = repository.paged,
        timerScope = viewModelScope,
        commitScope = appScope,
        commit = repository::remove,
    )
    val paging = PagingHandler(repository.paged, undo)

    init {
        launch { paging.state.collect { pages -> updateState { copy(pages = pages) } } }
        launch { paging.pendingUndo.collect { item -> updateState { copy(undo = item) } } }
    }

    override fun onCleared() {
        paging.close()
    }

    override fun handle(intent: HistoryIntent) {
        when (intent) {
            is HistoryIntent.Open -> open(intent.entry)
            is HistoryIntent.ToggleRead -> toggleRead(intent.key)
            HistoryIntent.ReadAll -> readAll()
            HistoryIntent.Clear -> clear()
            is HistoryIntent.Remove -> paging.dispatch(ListItemIntent.Remove(intent.key))
        }
    }

    private fun open(entry: NotificationHistory) {
        if (!entry.read) setRead(entry, true)
        entry.deepLink?.let { emit(HistoryEvent.OpenRoute(Route.Link(it))) }
    }

    private fun toggleRead(key: String) {
        val entry = currentState.pages.item(key) ?: return
        setRead(entry, !entry.read)
    }

    private fun setRead(entry: NotificationHistory, read: Boolean) {
        launch { repository.setRead(entry, read).failureOrNull()?.let { emit(HistoryEvent.Failure(it)) } }
    }

    private fun readAll() {
        launch { repository.readAll().failureOrNull()?.let { emit(HistoryEvent.Failure(it)) } }
    }

    private fun clear() {
        launch {
            val failure = repository.clear().failureOrNull()
            if (failure != null) emit(HistoryEvent.Failure(failure)) else emit(HistoryEvent.Cleared)
        }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(NotificationHistoryViewModel::class)
@Inject
class NotificationHistoryViewModelSource internal constructor(
    private val history: FeatureScope<NotificationHistoryRepository>,
    private val appScope: CoroutineScope,
) : ViewModelSource {
    override fun create(): ViewModel = history.viewModel { NotificationHistoryViewModel(it, appScope) }
}

package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.PagerRemoval
import dev.proflyder.home.core.data.PagingState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.PagingDelegate
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.time.Instant
import java.time.ZoneId
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay

internal data class NotificationHistoryUiState(
    val paging: PagingState<NotificationHistory> = PagingState(),
    val rows: List<HistoryRow> = emptyList(),
    val undo: NotificationHistory? = null,
) {
    val unread: Int get() = paging.items.count { !it.read }
}

internal sealed interface HistoryIntent {
    data object Refresh : HistoryIntent
    data object LoadMore : HistoryIntent
    data class Open(val entry: NotificationHistory) : HistoryIntent
    data object ReadAll : HistoryIntent
    data class Delete(val entry: NotificationHistory) : HistoryIntent
    data class ToggleRead(val entry: NotificationHistory) : HistoryIntent
    data object Undo : HistoryIntent
    data object Clear : HistoryIntent
}

internal sealed interface HistoryEvent {
    data object Cleared : HistoryEvent
    data class OpenRoute(val route: Route) : HistoryEvent
    data class Failure(val failure: RemoteFailure) : HistoryEvent
}

internal class NotificationHistoryViewModel(
    private val repository: NotificationHistoryRepository,
    private val clock: Clock,
) : StateViewModel<NotificationHistoryUiState, HistoryIntent, HistoryEvent>(NotificationHistoryUiState()) {
    private val zone: ZoneId = ZoneId.systemDefault()
    private val paging = PagingDelegate(repository.pager, viewModelScope)
    private var pendingDelete: PagerRemoval<NotificationHistory>? = null
    private var deleteJob: Job? = null

    init {
        paging.follow { state -> updateState { copy(paging = state, rows = rowsOf(state.items)) } }
    }

    private fun rowsOf(entries: List<NotificationHistory>): List<HistoryRow> =
        HistoryRows.of(entries, Instant.ofEpochMilli(clock.wallMillis()).atZone(zone).toLocalDate(), zone)

    override fun handle(intent: HistoryIntent) {
        when (intent) {
            HistoryIntent.Refresh -> paging.refresh(RefreshMode.HARD)
            HistoryIntent.LoadMore -> paging.loadMore()
            is HistoryIntent.Open -> open(intent.entry)
            HistoryIntent.ReadAll -> readAll()
            is HistoryIntent.Delete -> delete(intent.entry)
            is HistoryIntent.ToggleRead -> toggleRead(intent.entry)
            HistoryIntent.Undo -> undo()
            HistoryIntent.Clear -> clear()
        }
    }

    private fun open(entry: NotificationHistory) {
        if (!entry.read) changeRead(entry, true)
        entry.deepLink?.let { emit(HistoryEvent.OpenRoute(Route.Link(it))) }
    }

    private fun readAll() {
        launch {
            repository.readAll().failureOrNull()?.let { emit(HistoryEvent.Failure(it)) }
        }
    }

    private fun toggleRead(entry: NotificationHistory) {
        changeRead(entry, !entry.read)
    }

    private fun changeRead(entry: NotificationHistory, read: Boolean) {
        launch {
            repository.setRead(entry, read).failureOrNull()?.let { emit(HistoryEvent.Failure(it)) }
        }
    }

    private fun delete(entry: NotificationHistory) {
        val removal = repository.removeLocal(entry) ?: return
        pendingDelete = removal
        updateState { copy(undo = entry) }
        deleteJob?.cancel()
        deleteJob = launch {
            delay(UNDO_WINDOW_MILLIS)
            commitDelete()
        }
    }

    private suspend fun commitDelete() {
        val removal = pendingDelete ?: return
        pendingDelete = null
        updateState { copy(undo = null) }
        repository.remove(removal.item).failureOrNull()?.let { failure ->
            repository.pager.refresh(RefreshMode.HARD)
            emit(HistoryEvent.Failure(failure))
        }
    }

    override fun onCleared() {
        val removal = pendingDelete ?: return
        pendingDelete = null
        repository.removeLater(removal.item)
    }

    private fun undo() {
        deleteJob?.cancel()
        deleteJob = null
        val removal = pendingDelete ?: return
        pendingDelete = null
        repository.restoreLocal(removal)
        updateState { copy(undo = null) }
    }

    private fun clear() {
        launch {
            repository.clear().failureOrNull()?.let { failure ->
                emit(HistoryEvent.Failure(failure))
                return@launch
            }
            emit(HistoryEvent.Cleared)
        }
    }

    private companion object {
        const val UNDO_WINDOW_MILLIS = 6_000L
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(NotificationHistoryViewModel::class)
@Inject
class NotificationHistoryViewModelSource internal constructor(
    private val history: FeatureScope<NotificationHistoryRepository>,
    private val clock: Clock,
) : ViewModelSource {
    override fun create(): ViewModel = history.viewModel { NotificationHistoryViewModel(it, clock) }
}

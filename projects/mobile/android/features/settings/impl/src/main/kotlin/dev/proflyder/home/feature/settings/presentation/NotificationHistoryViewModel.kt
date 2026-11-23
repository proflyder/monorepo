package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.UndoWindow
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.push.NotificationHistoryRules
import dev.proflyder.home.feature.push.NotificationHistoryStore
import dev.proflyder.home.core.ui.ViewModelSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.time.Instant
import java.time.ZoneId

internal data class NotificationHistoryUiState(
    val entries: List<NotificationHistoryEntry> = emptyList(),
    val rows: List<HistoryRow> = emptyList(),
    val undo: NotificationHistoryEntry? = null,
) {
    val isEmpty: Boolean get() = entries.isEmpty()
    val unread: Int get() = NotificationHistoryRules.unread(entries)
}

internal sealed interface HistoryIntent {
    data class Open(val entry: NotificationHistoryEntry) : HistoryIntent
    data object ReadAll : HistoryIntent
    data class Delete(val entry: NotificationHistoryEntry) : HistoryIntent
    data class ToggleRead(val entry: NotificationHistoryEntry) : HistoryIntent
    data object Undo : HistoryIntent
    data object Clear : HistoryIntent
}

internal sealed interface HistoryEvent {
    data object Cleared : HistoryEvent
    data class OpenRoute(val route: Route) : HistoryEvent
}

internal class NotificationHistoryViewModel(
    private val store: NotificationHistoryStore,
    private val clock: Clock,
) : StateViewModel<NotificationHistoryUiState, HistoryIntent, HistoryEvent>(NotificationHistoryUiState()) {
    private val zone: ZoneId = ZoneId.systemDefault()
    private val window = UndoWindow<NotificationHistoryEntry>(viewModelScope)

    init {
        launch { store.entries.collect { stored -> updateState { copy(entries = stored, rows = rowsOf(stored)) } } }
        launch { window.pending.collect { pending -> updateState { copy(undo = pending) } } }
    }

    private fun rowsOf(entries: List<NotificationHistoryEntry>): List<HistoryRow> =
        HistoryRows.of(entries, Instant.ofEpochMilli(clock.wallMillis()).atZone(zone).toLocalDate(), zone)

    override fun handle(intent: HistoryIntent) {
        when (intent) {
            is HistoryIntent.Open -> open(intent.entry)
            HistoryIntent.ReadAll -> launch { store.markAllRead() }
            is HistoryIntent.Delete -> delete(intent.entry)
            is HistoryIntent.ToggleRead -> launch { store.setRead(intent.entry.id, !intent.entry.read) }
            HistoryIntent.Undo -> window.take()?.let { entry -> launch { store.restore(entry) } }
            HistoryIntent.Clear -> clear()
        }
    }

    private fun open(entry: NotificationHistoryEntry) {
        if (!entry.read) launch { store.setRead(entry.id, true) }
        entry.deepLink?.let { emit(HistoryEvent.OpenRoute(Route.Link(it))) }
    }

    private fun delete(entry: NotificationHistoryEntry) {
        launch { store.delete(entry.id) }
        window.open(entry)
    }

    private fun clear() {
        launch {
            store.clear()
            emit(HistoryEvent.Cleared)
        }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(NotificationHistoryViewModel::class)
@Inject
class NotificationHistoryViewModelSource internal constructor(
    private val history: FeatureScope<NotificationHistoryStore>,
    private val clock: Clock,
) : ViewModelSource {
    override fun create(): ViewModel = history.viewModel { NotificationHistoryViewModel(it, clock) }
}

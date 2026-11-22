package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.feature.push.NotificationHistoryEntry
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
) {
    val isEmpty: Boolean get() = entries.isEmpty()
}

internal sealed interface HistoryIntent {
    data class Open(val entry: NotificationHistoryEntry) : HistoryIntent
    data object Clear : HistoryIntent
}

internal sealed interface HistoryEvent {
    data object Cleared : HistoryEvent
    data class OpenRoute(val route: Route) : HistoryEvent
}

@Inject
internal class NotificationHistoryViewModel(
    private val store: NotificationHistoryStore,
    private val clock: Clock,
) : StateViewModel<NotificationHistoryUiState, HistoryIntent, HistoryEvent>(NotificationHistoryUiState()) {
    private val zone: ZoneId = ZoneId.systemDefault()

    init {
        launch { store.load() }
        launch { store.entries.collect { stored -> updateState { copy(entries = stored, rows = rowsOf(stored)) } } }
    }

    private fun rowsOf(entries: List<NotificationHistoryEntry>): List<HistoryRow> =
        HistoryRows.of(entries, Instant.ofEpochMilli(clock.wallMillis()).atZone(zone).toLocalDate(), zone)

    override fun handle(intent: HistoryIntent) {
        when (intent) {
            is HistoryIntent.Open -> intent.entry.deepLink?.let { emit(HistoryEvent.OpenRoute(Route.Link(it))) }
            HistoryIntent.Clear -> clear()
        }
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
    private val viewModel: () -> NotificationHistoryViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.push.NotificationHistoryStore
import dev.proflyder.home.core.ui.ViewModelSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

data class NotificationHistoryUiState(
    val entries: List<NotificationHistoryEntry> = emptyList(),
) {
    val isEmpty: Boolean get() = entries.isEmpty()
}

internal sealed interface HistoryIntent {
    data class Open(val entry: NotificationHistoryEntry) : HistoryIntent
    data object Clear : HistoryIntent
}

internal sealed interface HistoryEvent {
    data object Cleared : HistoryEvent
    data class OpenRoute(val route: String) : HistoryEvent
}

@Inject
internal class NotificationHistoryViewModel(
    private val store: NotificationHistoryStore,
) : StateViewModel<NotificationHistoryUiState, HistoryIntent, HistoryEvent>(
    NotificationHistoryUiState(entries = store.entries.value),
) {
    init {
        launch { store.load() }
        launch { store.entries.collect { stored -> updateState { copy(entries = stored) } } }
    }

    override fun handle(intent: HistoryIntent) {
        when (intent) {
            is HistoryIntent.Open -> intent.entry.deepLink?.let { emit(HistoryEvent.OpenRoute(it)) }
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

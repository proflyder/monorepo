package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.push.NotificationHistoryStore
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

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

@ContributesIntoSet(AppScope::class)
@Inject
class NotificationHistoryViewModelFactoryContributor(
    private val store: NotificationHistoryStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = NotificationHistoryViewModel::class.java

    override fun createFactory(): ViewModelProvider.Factory = SimpleViewModelFactory {
        NotificationHistoryViewModel(store)
    }
}

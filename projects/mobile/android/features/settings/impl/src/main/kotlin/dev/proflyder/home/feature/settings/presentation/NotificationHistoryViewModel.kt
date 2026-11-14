package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.push.NotificationHistoryStore
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

/** One-shot confirmations the history screen shows and then forgets. */
enum class HistoryNotice {
    CLEARED,
}

data class NotificationHistoryUiState(
    val entries: List<NotificationHistoryEntry> = emptyList(),
    val notice: HistoryNotice? = null,
) {
    val isEmpty: Boolean get() = entries.isEmpty()
}

/**
 * Reads the local log only: the history is what the phone received, so it never
 * asks the server (which may well be unreachable right now).
 */
class NotificationHistoryViewModel(
    private val store: NotificationHistoryStore,
) : BaseViewModel<NotificationHistoryUiState>(
    NotificationHistoryUiState(entries = store.entries.value),
) {
    init {
        launch { store.load() }
        launch { store.entries.collect { stored -> updateState { copy(entries = stored) } } }
    }

    fun clear() {
        launch {
            store.clear()
            updateState { copy(notice = HistoryNotice.CLEARED) }
        }
    }

    fun consumeNotice() {
        updateState { copy(notice = null) }
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class NotificationHistoryViewModelFactoryContributor(
    private val store: NotificationHistoryStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = NotificationHistoryViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        NotificationHistoryViewModel(store)
    }
}

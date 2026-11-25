package dev.proflyder.home.feature.chat.presentation.sessions

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.feature.chat.ChatSessionsDestination
import dev.proflyder.home.feature.chat.data.ChatComponents
import dev.proflyder.home.feature.chat.data.ChatSessionsRepository
import dev.proflyder.home.feature.chat.data.SessionAction
import dev.proflyder.home.feature.chat.data.SessionActions
import dev.proflyder.home.feature.chat.data.SessionsSnapshot
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class ChatSessionsUiState(
    val archived: Boolean = false,
    val sessions: List<ChatSession> = emptyList(),
    val archivedCount: Int = 0,
    val query: String = "",
    val loaded: Boolean = false,
    val failure: RemoteFailure? = null,
    val creating: Boolean = false,
) {
    val visible: List<ChatSession> get() = sessions.filter { it.matches(query) }
}

internal fun ChatSession.matches(query: String): Boolean {
    val needle = query.trim()
    if (needle.isEmpty()) return true
    return title.orEmpty().contains(needle, ignoreCase = true) || preview?.text.orEmpty().contains(needle, ignoreCase = true)
}

internal sealed interface ChatSessionsIntent {
    data object Refresh : ChatSessionsIntent
    data object Create : ChatSessionsIntent
    data class Search(val query: String) : ChatSessionsIntent
    data class Act(val action: SessionAction) : ChatSessionsIntent
}

internal sealed interface ChatSessionsEvent {
    data class Open(val sessionId: String) : ChatSessionsEvent
    data class Failure(val failure: RemoteFailure) : ChatSessionsEvent
}

internal class ChatSessionsViewModel(
    private val repository: ChatSessionsRepository,
    private val actions: SessionActions,
    archived: Boolean,
) : StateViewModel<ChatSessionsUiState, ChatSessionsIntent, ChatSessionsEvent>(ChatSessionsUiState(archived = archived)) {
    init {
        repository.sessions.onEach(::onSnapshot).launchIn(viewModelScope)
        launch { report(repository.refresh()) }
    }

    override fun handle(intent: ChatSessionsIntent) {
        when (intent) {
            ChatSessionsIntent.Refresh -> launch { report(repository.refresh()) }
            ChatSessionsIntent.Create -> create()
            is ChatSessionsIntent.Search -> updateState { copy(query = intent.query) }
            is ChatSessionsIntent.Act -> launch { report(actions.perform(intent.action)) }
        }
    }

    private fun onSnapshot(snapshot: SessionsSnapshot) = updateState {
        copy(
            sessions = if (archived) snapshot.archived else snapshot.active,
            archivedCount = snapshot.archived.size,
            loaded = snapshot.loaded,
            failure = snapshot.failure,
        )
    }

    private fun create() {
        if (currentState.creating) return
        updateState { copy(creating = true) }
        launch { opened(repository.create()) }
    }

    private fun opened(result: Outcome<ChatSession>) {
        updateState { copy(creating = false) }
        when (result) {
            is Outcome.Success -> emit(ChatSessionsEvent.Open(result.value.id))
            is Outcome.Failure -> emit(ChatSessionsEvent.Failure(result.failure))
        }
    }

    private fun report(outcome: Outcome<Unit>) {
        if (outcome is Outcome.Failure) emit(ChatSessionsEvent.Failure(outcome.failure))
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<SavedStateViewModelFactory>())
@ClassKey(ChatSessionsViewModel::class)
@Inject
class ChatSessionsViewModelSource internal constructor(
    private val components: ChatComponents,
) : SavedStateViewModelFactory {
    override fun create(savedState: SavedStateHandle): ViewModel = ChatSessionsViewModel(
        repository = components.sessions,
        actions = components.sessionActions,
        archived = savedState.destination(ChatSessionsDestination)?.archived == true,
    )
}

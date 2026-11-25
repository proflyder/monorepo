package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.model.ChatSessionPatch
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.core.network.StreamUnauthorizedException
import dev.proflyder.home.core.runtime.Backoff
import dev.proflyder.home.core.runtime.reconnecting
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transform
import kotlinx.coroutines.flow.update

internal object ChatSessions {
    const val LEGACY_ID = "home-app"
    const val LIVE_ONLY_CURSOR = Long.MAX_VALUE
    const val REFRESH_DEBOUNCE_MILLIS = 400L
}

internal data class SessionsSnapshot(
    val active: List<ChatSession> = emptyList(),
    val archived: List<ChatSession> = emptyList(),
    val loaded: Boolean = false,
    val failure: RemoteFailure? = null,
) {
    fun find(id: String): ChatSession? = active.firstOrNull { it.id == id } ?: archived.firstOrNull { it.id == id }

    fun with(session: ChatSession): SessionsSnapshot {
        val others = without(session.id)
        return when (session.archived) {
            true -> others.copy(archived = (others.archived + session).ordered())
            false -> others.copy(active = (others.active + session).ordered())
        }
    }

    fun without(id: String): SessionsSnapshot =
        copy(active = active.filterNot { it.id == id }, archived = archived.filterNot { it.id == id })
}

internal fun List<ChatSession>.ordered(): List<ChatSession> =
    sortedWith(compareByDescending<ChatSession> { it.pinned }.thenByDescending { it.updatedAtMillis })

internal interface ChatSessionsRepository {
    val sessions: StateFlow<SessionsSnapshot>

    suspend fun refresh(): Outcome<Unit>

    suspend fun create(): Outcome<ChatSession>

    suspend fun update(id: String, patch: ChatSessionPatch): Outcome<ChatSession>

    suspend fun delete(id: String): Outcome<Unit>
}

internal class LiveChatSessionsRepository(
    private val client: ChatClient,
    private val scope: CoroutineScope,
    private val backoff: Backoff = Backoff(),
) : ChatSessionsRepository {
    private val state = MutableStateFlow(SessionsSnapshot())

    override val sessions: StateFlow<SessionsSnapshot> by lazy {
        merge(state, liveUpdates()).stateIn(scope, SharingStarted.WhileSubscribed(LIVE_STOP_MILLIS), state.value)
    }

    override suspend fun refresh(): Outcome<Unit> = coroutineScope {
        val active = async { client.sessions(archived = false) }
        val archived = async { client.sessions(archived = true) }
        combineLists(active.await(), archived.await())
    }

    private fun combineLists(active: Outcome<List<ChatSession>>, archived: Outcome<List<ChatSession>>): Outcome<Unit> {
        val failure = (active as? Outcome.Failure ?: archived as? Outcome.Failure)
        if (failure != null) return failure.also { state.update { it.copy(failure = failure.failure) } }
        val activeList = (active as Outcome.Success).value
        val archivedList = (archived as Outcome.Success).value
        state.value = SessionsSnapshot(activeList.ordered(), archivedList.ordered(), loaded = true)
        return Outcome.Success(Unit)
    }

    override suspend fun create(): Outcome<ChatSession> = client.createSession(title = null).also(::remember)

    override suspend fun update(id: String, patch: ChatSessionPatch): Outcome<ChatSession> =
        client.patchSession(id, patch).also(::remember)

    override suspend fun delete(id: String): Outcome<Unit> = client.deleteSession(id).also { result ->
        if (result is Outcome.Success) state.update { it.without(id) }
    }

    private fun remember(result: Outcome<ChatSession>) {
        if (result is Outcome.Success) state.update { it.with(result.value) }
    }

    @OptIn(FlowPreview::class)
    private fun liveUpdates(): Flow<Nothing> {
        val events = flow { emitAll(connectEvents()) }
            .reconnecting(backoff) { it !is StreamUnauthorizedException }
            .catch { if (it !is StreamUnauthorizedException) throw it }
            .onEach(::applySession)
        return events.filter { it is ChatEvent.Upsert }
            .debounce(ChatSessions.REFRESH_DEBOUNCE_MILLIS)
            .transform { refresh() }
    }

    private suspend fun connectEvents(): Flow<ChatEvent> {
        if (!state.value.loaded) refresh()
        return client.events(sessionId = null, after = ChatSessions.LIVE_ONLY_CURSOR)
    }

    private fun applySession(event: ChatEvent) {
        when (event) {
            is ChatEvent.SessionChanged -> state.update { it.with(event.session) }
            is ChatEvent.SessionRemoved -> state.update { it.without(event.id) }
            is ChatEvent.Upsert, is ChatEvent.Delta, is ChatEvent.Tool -> Unit
        }
    }
}

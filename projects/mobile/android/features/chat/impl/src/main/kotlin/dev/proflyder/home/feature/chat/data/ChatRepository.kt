package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.core.network.ChatUnauthorizedException
import dev.proflyder.home.core.network.NetworkWaiter
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.UUID
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

internal class ChatClock(
    val now: () -> Long = System::currentTimeMillis,
    val newClientId: () -> String = { UUID.randomUUID().toString() },
    val initialBackoffMillis: Long = INITIAL_BACKOFF_MILLIS,
    val maxBackoffMillis: Long = MAX_BACKOFF_MILLIS,
    val io: CoroutineDispatcher = Dispatchers.IO,
) {
    companion object {
        const val INITIAL_BACKOFF_MILLIS = 1_000L
        const val MAX_BACKOFF_MILLIS = 30_000L
    }
}

internal const val CHAT_PAGE_SIZE = 50
internal const val NETWORK_WAIT_MILLIS = 30_000L
internal const val FAILURE_UNAUTHORIZED = "chat_unauthorized"
internal const val FAILURE_NOT_SENT = "chat_not_sent"

private val PERMANENT_SEND_CODES = setOf(
    "chat_text_empty",
    "chat_text_too_long",
    "chat_client_id_invalid",
    "chat_reply_not_found",
)

internal fun RemoteFailure.sendFailureKey(): String? = when (this) {
    is RemoteFailure.Offline, RemoteFailure.Timeout, is RemoteFailure.Tls, is RemoteFailure.ProtocolFailure -> null
    is RemoteFailure.Server -> code.takeIf { it in PERMANENT_SEND_CODES }
    RemoteFailure.Unauthorized -> FAILURE_UNAUTHORIZED
    RemoteFailure.NotConfigured, is RemoteFailure.Validation, is RemoteFailure.Storage -> FAILURE_NOT_SENT
}

@SingleIn(AppScope::class)
@Inject
class ChatRepository(
    private val client: ChatClient,
    private val outboxStore: ChatOutboxStore,
    private val scope: CoroutineScope,
    private val networkWaiter: NetworkWaiter,
) {
    internal var clock: ChatClock = ChatClock()

    private val state = MutableStateFlow(ChatTimeline())
    private val flushLock = Mutex()
    private val restoreLock = Mutex()
    private var restored = false

    internal val timeline: StateFlow<ChatTimeline> = state.asStateFlow()

    suspend fun refresh(): Outcome<Unit> {
        restoreOutbox()
        return when (val page = client.page(before = null, limit = CHAT_PAGE_SIZE)) {
            is Outcome.Success -> Outcome.Success(Unit).also { onPage(page.value) }
            is Outcome.Failure -> page.also { failure -> state.update { it.copy(failure = failure.failure) } }
        }
    }

    private fun onPage(page: ChatPage) {
        state.update { it.withPage(page) }
        kick()
    }

    suspend fun loadOlder() {
        val current = state.value
        val oldest = current.oldestSeq ?: return
        if (!current.hasMore || current.loadingOlder) return
        state.update { it.copy(loadingOlder = true) }
        when (val page = client.page(before = oldest, limit = CHAT_PAGE_SIZE)) {
            is Outcome.Success -> state.update { it.withOlder(page.value) }
            is Outcome.Failure -> state.update { it.copy(loadingOlder = false) }
        }
    }

    fun send(text: String, replyTo: ChatQuote?) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return
        val outgoing = Outgoing(clock.newClientId(), trimmed, replyTo, clock.now())
        state.update { it.enqueue(outgoing) }
        persistAndKick()
    }

    fun retryOutgoing(clientId: String) {
        state.update { it.clearFailure(clientId) }
        persistAndKick()
    }

    fun discardOutgoing(clientId: String) {
        state.update { it.remove(clientId) }
        scope.launch { persist() }
    }

    suspend fun retryReply(id: String): Outcome<Unit> = when (val result = client.retry(id)) {
        is Outcome.Success -> Outcome.Success(Unit).also { accept(result.value) }
        is Outcome.Failure -> result
    }

    private fun accept(message: ChatMessage) = state.update { it.upsert(message) }

    suspend fun runLive() {
        var wait = clock.initialBackoffMillis
        while (true) {
            if (!state.value.loaded) refresh()
            val outcome = suspendRunCatching { streamOnce() }
            if (outcome.exceptionOrNull() is ChatUnauthorizedException) return
            wait = if (outcome.isSuccess) clock.initialBackoffMillis else nextBackoff(wait)
            delay(wait)
        }
    }

    private suspend fun streamOnce() {
        client.events(state.value.cursor).collect { event -> state.update { it.apply(event) } }
    }

    private fun persistAndKick() {
        scope.launch {
            persist()
            flush()
        }
    }

    private fun kick() {
        scope.launch { flush() }
    }

    private suspend fun flush() = flushLock.withLock { drain() }

    private suspend fun drain() {
        var wait = clock.initialBackoffMillis
        while (true) {
            val next = state.value.outbox.firstOrNull { !it.failed } ?: return
            wait = deliver(next, wait)
        }
    }

    private suspend fun deliver(next: Outgoing, wait: Long): Long =
        when (val result = client.send(next.clientId, next.text, next.replyTo?.id)) {
            is Outcome.Success -> clock.initialBackoffMillis.also { delivered(result.value) }
            is Outcome.Failure -> undelivered(next, result.failure, wait)
        }

    private suspend fun delivered(sent: ChatSent) {
        state.update { it.accept(sent) }
        persist()
    }

    private suspend fun undelivered(next: Outgoing, failure: RemoteFailure, wait: Long): Long {
        val key = failure.sendFailureKey() ?: return nextBackoff(wait).also { awaitNetwork(wait) }
        state.update { it.markFailed(next.clientId, key) }
        persist()
        return wait
    }

    private suspend fun awaitNetwork(wait: Long) {
        withContext(clock.io) { networkWaiter.awaitAvailable(NETWORK_WAIT_MILLIS) }
        delay(wait)
    }

    private fun nextBackoff(wait: Long): Long = (wait * 2).coerceAtMost(clock.maxBackoffMillis)

    private suspend fun restoreOutbox() = restoreLock.withLock { restoreOnce() }

    private suspend fun restoreOnce() {
        if (restored) return
        restored = true
        val saved = outboxStore.load().map { it.toOutgoing() }
        state.update { current -> current.copy(outbox = saved + current.outbox) }
    }

    private suspend fun persist() {
        outboxStore.save(state.value.outbox.map { it.toRecord() })
    }
}

package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatAttachment
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.StorageReason
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.core.network.ChatUnauthorizedException
import dev.proflyder.home.core.network.NetworkWaiter
import dev.proflyder.home.core.runtime.Backoff
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.retryWhen
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlin.time.Duration.Companion.milliseconds

internal class ChatClock(
    val now: () -> Long = System::currentTimeMillis,
    val newClientId: () -> String = { UUID.randomUUID().toString() },
    val initialBackoffMillis: Long = INITIAL_BACKOFF_MILLIS,
    val maxBackoffMillis: Long = MAX_BACKOFF_MILLIS,
    val io: CoroutineDispatcher = Dispatchers.IO,
) {
    val backoff: Backoff get() = Backoff(initialBackoffMillis.milliseconds, maxBackoffMillis.milliseconds)

    companion object {
        const val INITIAL_BACKOFF_MILLIS = 1_000L
        const val MAX_BACKOFF_MILLIS = 30_000L
    }
}

internal const val CHAT_PAGE_SIZE = 50
internal const val LIVE_STOP_MILLIS = 5_000L
internal const val NETWORK_WAIT_MILLIS = 30_000L
internal const val FAILURE_UNAUTHORIZED = "chat_unauthorized"
internal const val FAILURE_NOT_SENT = "chat_not_sent"

private val PERMANENT_SEND_CODES = setOf(
    "chat_text_empty",
    "chat_text_too_long",
    "chat_client_id_invalid",
    "chat_reply_not_found",
    "chat_attachment_too_large",
    "chat_attachment_unsupported",
    "chat_attachment_not_found",
    "chat_too_many_attachments",
    "chat_voice_format",
)

internal const val FAILURE_FILE_MISSING = "chat_attachment_missing"

internal fun RemoteFailure.sendFailureKey(): String? = when (this) {
    is RemoteFailure.Offline, RemoteFailure.Timeout, is RemoteFailure.Tls, is RemoteFailure.ProtocolFailure -> null
    is RemoteFailure.Server -> code.takeIf { it in PERMANENT_SEND_CODES }
    RemoteFailure.Unauthorized -> FAILURE_UNAUTHORIZED
    is RemoteFailure.Storage -> reason.sendFailureKey()
    RemoteFailure.NotConfigured, is RemoteFailure.Validation -> FAILURE_NOT_SENT
}

private fun StorageReason.sendFailureKey(): String = when (this) {
    StorageReason.ATTACHMENT_MISSING -> FAILURE_FILE_MISSING
    StorageReason.TOKEN_ENCRYPTION, StorageReason.SETTINGS_WRITE -> FAILURE_NOT_SENT
}

internal interface ChatRepository {
    val timeline: StateFlow<ChatTimeline>

    suspend fun refresh(): Outcome<Unit>

    suspend fun loadOlder()

    fun send(text: String, replyTo: ChatQuote?, attachments: List<LocalAttachment> = emptyList())

    fun retryOutgoing(clientId: String)

    fun discardOutgoing(clientId: String)

    suspend fun retryReply(id: String): Outcome<Unit>
}

internal class LiveChatRepository(
    private val client: ChatClient,
    private val outboxStore: ChatOutboxStore,
    private val scope: CoroutineScope,
    private val networkWaiter: NetworkWaiter,
) : ChatRepository {
    internal var clock: ChatClock = ChatClock()

    private val state = MutableStateFlow(ChatTimeline())
    private val flushLock = Mutex()
    private val restoreLock = Mutex()
    private var restored = false

    override val timeline: StateFlow<ChatTimeline> by lazy {
        merge(state, liveUpdates()).stateIn(scope, SharingStarted.WhileSubscribed(LIVE_STOP_MILLIS), state.value)
    }

    internal val snapshot: ChatTimeline get() = state.value

    override suspend fun refresh(): Outcome<Unit> {
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

    override suspend fun loadOlder() {
        val current = state.value
        val oldest = current.oldestSeq ?: return
        if (!current.hasMore || current.loadingOlder) return
        state.update { it.copy(loadingOlder = true) }
        when (val page = client.page(before = oldest, limit = CHAT_PAGE_SIZE)) {
            is Outcome.Success -> state.update { it.withOlder(page.value) }
            is Outcome.Failure -> state.update { it.copy(loadingOlder = false) }
        }
    }

    override fun send(text: String, replyTo: ChatQuote?, attachments: List<LocalAttachment>) {
        val trimmed = text.trim()
        if (trimmed.isEmpty() && attachments.isEmpty()) return
        val outgoing = Outgoing(clock.newClientId(), trimmed, replyTo, clock.now(), attachments = attachments)
        state.update { it.enqueue(outgoing) }
        persistAndKick()
    }

    override fun retryOutgoing(clientId: String) {
        state.update { it.clearFailure(clientId) }
        persistAndKick()
    }

    override fun discardOutgoing(clientId: String) {
        state.update { it.remove(clientId) }
        scope.launch { persist() }
    }

    override suspend fun retryReply(id: String): Outcome<Unit> = when (val result = client.retry(id)) {
        is Outcome.Success -> Outcome.Success(Unit).also { accept(result.value) }
        is Outcome.Failure -> result
    }

    private fun accept(message: ChatMessage) = state.update { it.upsert(message) }

    private fun liveUpdates(): Flow<Nothing> {
        var failures = 0L
        return flow<Nothing> { streamOnce { failures = 0 } }
            .retryWhen { cause, _ -> shouldReconnect(cause, failures++) }
            .catch { }
    }

    private suspend fun shouldReconnect(cause: Throwable, failures: Long): Boolean {
        if (cause is ChatUnauthorizedException) return false
        delay(if (cause is StreamClosed) clock.backoff.initial else clock.backoff.delayFor(failures))
        return true
    }

    private suspend fun streamOnce(onEvent: () -> Unit) {
        if (!state.value.loaded) refresh()
        client.events(state.value.cursor).collect { event ->
            onEvent()
            state.update { it.apply(event) }
        }
        throw StreamClosed()
    }

    private class StreamClosed : IllegalStateException("chat stream closed")

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

    private suspend fun deliver(next: Outgoing, wait: Long): Long {
        val pending = next.attachments.firstOrNull { it.uploadedId == null }
            ?: return sendUploaded(next, wait)
        return when (val result = upload(pending)) {
            is Outcome.Success -> wait.also { attached(next, pending, result.value.id) }
            is Outcome.Failure -> undelivered(next, result.failure, wait)
        }
    }

    private suspend fun sendUploaded(next: Outgoing, wait: Long): Long {
        val ids = next.attachments.mapNotNull { it.uploadedId }
        return when (val result = client.send(next.clientId, next.text, next.replyTo?.id, ids)) {
            is Outcome.Success -> clock.initialBackoffMillis.also { delivered(result.value) }
            is Outcome.Failure -> undelivered(next, result.failure, wait)
        }
    }

    private suspend fun upload(attachment: LocalAttachment): Outcome<ChatAttachment> {
        val bytes = withContext(clock.io) { File(attachment.path).takeIf { it.isFile }?.readBytes() }
            ?: return Outcome.Failure(RemoteFailure.Storage(StorageReason.ATTACHMENT_MISSING))
        return client.upload(bytes, attachment.mime, attachment.name)
    }

    private suspend fun attached(next: Outgoing, attachment: LocalAttachment, id: String) {
        state.update { it.uploaded(next.clientId, attachment.path, id) }
        persist()
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

package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.model.RemoteFailure

internal data class LocalAttachment(
    val path: String,
    val kind: ChatAttachmentKind,
    val mime: String,
    val name: String,
    val size: Long,
    val durationMs: Long? = null,
    val uploadedId: String? = null,
)

internal data class Outgoing(
    val clientId: String,
    val text: String,
    val replyTo: ChatQuote?,
    val createdAtMillis: Long,
    val failureKey: String? = null,
    val attachments: List<LocalAttachment> = emptyList(),
) {
    val failed: Boolean get() = failureKey != null
}

internal data class ChatTimeline(
    val messages: List<ChatMessage> = emptyList(),
    val outbox: List<Outgoing> = emptyList(),
    val cursor: Long = 0,
    val hasMore: Boolean = false,
    val loaded: Boolean = false,
    val loadingOlder: Boolean = false,
    val failure: RemoteFailure? = null,
    val tools: Map<String, String> = emptyMap(),
) {
    val activeTool: String? get() = tools.values.lastOrNull()

    val streamingReplyId: String? get() = messages.lastOrNull { it.isAnswering() }?.id

    val oldestSeq: Long? get() = messages.firstOrNull()?.seq

    fun withPage(page: ChatPage): ChatTimeline {
        val merged = (page.messages + messages).associateBy { it.id }.values.sortedBy { it.seq }
        return copy(
            messages = merged,
            cursor = maxOf(cursor, page.cursor),
            hasMore = page.hasMore,
            loaded = true,
            failure = null,
        ).dropDelivered()
    }

    fun withOlder(page: ChatPage): ChatTimeline =
        withPage(page).copy(cursor = cursor, loadingOlder = false)

    fun upsert(message: ChatMessage): ChatTimeline {
        val current = messages.firstOrNull { it.id == message.id }
        val newer = current == null || current.version <= message.version
        val replaced = when {
            newer -> messages.filterNot { it.id == message.id } + message
            else -> messages
        }
        return copy(
            messages = replaced.sortedBy { it.seq },
            cursor = maxOf(cursor, message.version),
            tools = if (message.isAnswering()) tools else tools - message.id,
        ).dropDelivered()
    }

    fun accept(sent: ChatSent): ChatTimeline = upsert(sent.message).upsert(sent.reply)

    fun apply(event: ChatEvent): ChatTimeline = when (event) {
        is ChatEvent.Upsert -> upsert(event.message)
        is ChatEvent.Delta -> appendDelta(event)
        is ChatEvent.Tool -> withTool(event)
        is ChatEvent.SessionChanged, is ChatEvent.SessionRemoved -> this
    }

    private fun withTool(tool: ChatEvent.Tool): ChatTimeline =
        copy(tools = if (tool.done) tools - tool.id else tools + (tool.id to tool.name))

    private fun appendDelta(delta: ChatEvent.Delta): ChatTimeline {
        val target = messages.firstOrNull { it.id == delta.id && it.text.length == delta.offset }
            ?: return this
        val grown = target.copy(text = target.text + delta.text, status = ChatStatus.STREAMING)
        return copy(messages = messages.map { if (it.id == grown.id) grown else it })
    }

    fun enqueue(outgoing: Outgoing): ChatTimeline = copy(outbox = outbox + outgoing)

    fun markFailed(clientId: String, key: String): ChatTimeline =
        copy(outbox = outbox.map { if (it.clientId == clientId) it.copy(failureKey = key) else it })

    fun clearFailure(clientId: String): ChatTimeline =
        copy(outbox = outbox.map { if (it.clientId == clientId) it.copy(failureKey = null) else it })

    fun uploaded(clientId: String, path: String, id: String): ChatTimeline = copy(
        outbox = outbox.map { item ->
            if (item.clientId == clientId) item.withUploaded(path, id) else item
        },
    )

    fun remove(clientId: String): ChatTimeline = copy(outbox = outbox.filterNot { it.clientId == clientId })

    private fun dropDelivered(): ChatTimeline {
        val delivered = messages.mapNotNullTo(HashSet()) { it.clientId }
        return copy(outbox = outbox.filterNot { it.clientId in delivered })
    }
}

internal fun ChatMessage.isAnswering(): Boolean =
    role == ChatRole.ASSISTANT && (status == ChatStatus.PENDING || status == ChatStatus.STREAMING)

internal fun Outgoing.withUploaded(path: String, id: String): Outgoing =
    copy(attachments = attachments.map { if (it.path == path) it.copy(uploadedId = id) else it })

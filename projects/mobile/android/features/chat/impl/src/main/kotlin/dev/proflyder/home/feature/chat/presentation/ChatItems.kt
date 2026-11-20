package dev.proflyder.home.feature.chat.presentation

import dev.proflyder.home.core.model.ChatAlert
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.feature.chat.data.ChatTimeline
import dev.proflyder.home.feature.chat.data.Outgoing
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

internal enum class BubbleState { QUEUED, SENT, STREAMING, DONE, FAILED }

internal data class BubbleMedia(
    val key: String,
    val id: String?,
    val path: String?,
    val kind: ChatAttachmentKind,
    val name: String,
    val size: Long,
    val durationMs: Long?,
)

internal data class VoicePlayback(
    val key: String,
    val positionMs: Long,
    val durationMs: Long,
    val playing: Boolean,
) {
    val progress: Float get() = if (durationMs <= 0) 0f else (positionMs.toFloat() / durationMs).coerceIn(0f, 1f)
}

internal interface Dated {
    val atMillis: Long
}

internal sealed interface ChatItem {
    val key: String

    data class Alert(
        override val key: String,
        val messageId: String,
        val alert: ChatAlert,
        val text: String,
        override val atMillis: Long,
    ) : ChatItem, Dated {
        fun asBubble(): Bubble = Bubble(
            key = key,
            messageId = messageId,
            clientId = null,
            mine = false,
            kind = ChatKind.ALERT,
            text = text,
            atMillis = atMillis,
            state = BubbleState.DONE,
            quote = null,
            errorKey = null,
        )
    }

    data class Day(val date: LocalDate) : ChatItem {
        override val key: String = "day-$date"
    }

    data class Bubble(
        override val key: String,
        val messageId: String?,
        val clientId: String?,
        val mine: Boolean,
        val kind: ChatKind,
        val text: String,
        override val atMillis: Long,
        val state: BubbleState,
        val quote: ChatQuote?,
        val errorKey: String?,
        val media: List<BubbleMedia> = emptyList(),
        val transcript: String? = null,
        val playback: VoicePlayback? = null,
    ) : ChatItem, Dated {
        val quotable: Boolean get() = messageId != null && text.isNotEmpty()
    }

    data object Typing : ChatItem {
        override val key: String = "typing"
    }

    data object Older : ChatItem {
        override val key: String = "older"
    }
}

internal object ChatItems {
    fun build(timeline: ChatTimeline, zone: ZoneId, playback: VoicePlayback? = null): List<ChatItem> {
        val bubbles = (timeline.messages.mapNotNull { it.toItem(playback) } + timeline.outbox.map { it.toBubble() })
        val items = mutableListOf<ChatItem>()
        if (timeline.loadingOlder) items += ChatItem.Older
        bubbles.fold(null as LocalDate?) { day, bubble -> items.addBubble(day, bubble, zone) }
        if (timeline.isTyping()) items += ChatItem.Typing
        return items
    }

    private fun MutableList<ChatItem>.addBubble(day: LocalDate?, bubble: ChatItem, zone: ZoneId): LocalDate {
        val date = Instant.ofEpochMilli((bubble as Dated).atMillis).atZone(zone).toLocalDate()
        if (date != day) add(ChatItem.Day(date))
        add(bubble)
        return date
    }

    private fun ChatMessage.toItem(playback: VoicePlayback?): ChatItem? {
        val card = alert ?: return toBubble()?.withPlayback(playback)
        return ChatItem.Alert("m-$id", id, card, text, createdAtMillis)
    }

    private fun ChatTimeline.isTyping(): Boolean = messages.any { it.awaitingFirstWords() }

    private fun ChatMessage.awaitingFirstWords(): Boolean =
        role == ChatRole.ASSISTANT && text.isEmpty() && (status == ChatStatus.PENDING || status == ChatStatus.STREAMING)

    private fun ChatMessage.toBubble(): ChatItem.Bubble? = when {
        awaitingFirstWords() -> null
        role == ChatRole.ASSISTANT && text.isEmpty() && status == ChatStatus.DONE -> null
        else -> ChatItem.Bubble(
            key = clientId?.let { "c-$it" } ?: "m-$id",
            messageId = id,
            clientId = clientId,
            mine = role == ChatRole.USER,
            kind = kind,
            text = text,
            atMillis = createdAtMillis,
            state = status.toState(role),
            quote = replyTo,
            errorKey = errorKey,
            media = attachments.map { BubbleMedia("a-${it.id}", it.id, null, it.kind, it.name, it.size, it.durationMs) },
            transcript = transcript,
        )
    }

    private fun ChatItem.Bubble.withPlayback(playback: VoicePlayback?): ChatItem.Bubble =
        if (playback != null && media.any { it.key == playback.key }) copy(playback = playback) else this

    private fun ChatStatus.toState(role: ChatRole): BubbleState = when (this) {
        ChatStatus.FAILED -> BubbleState.FAILED
        ChatStatus.STREAMING -> BubbleState.STREAMING
        ChatStatus.PENDING -> BubbleState.STREAMING
        ChatStatus.DONE -> if (role == ChatRole.USER) BubbleState.SENT else BubbleState.DONE
    }

    private fun Outgoing.toBubble(): ChatItem.Bubble = ChatItem.Bubble(
        key = "c-$clientId",
        messageId = null,
        clientId = clientId,
        mine = true,
        kind = ChatKind.TEXT,
        text = text,
        atMillis = createdAtMillis,
        state = if (failed) BubbleState.FAILED else BubbleState.QUEUED,
        quote = replyTo,
        errorKey = failureKey,
        media = attachments.map { BubbleMedia("l-${it.path}", it.uploadedId, it.path, it.kind, it.name, it.size, it.durationMs) },
    )
}

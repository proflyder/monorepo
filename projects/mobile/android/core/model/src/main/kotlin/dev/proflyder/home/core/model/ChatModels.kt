package dev.proflyder.home.core.model

enum class ChatRole { USER, ASSISTANT, SYSTEM }

enum class ChatKind { TEXT, ALERT, VOICE, IMAGE, FILE }

enum class ChatAttachmentKind { VOICE, IMAGE, FILE }

data class ChatAttachment(
    val id: String,
    val kind: ChatAttachmentKind,
    val mime: String,
    val size: Long,
    val name: String,
    val durationMs: Long?,
)

data class ChatAlert(
    val fingerprint: String,
    val name: String,
    val severity: String,
    val resolved: Boolean,
    val summary: String,
    val description: String,
    val namespace: String,
    val target: String,
    val startsAtMillis: Long,
    val endsAtMillis: Long?,
    val runbookUrl: String,
)

enum class ChatStatus { PENDING, STREAMING, DONE, FAILED }

data class ChatQuote(
    val id: String,
    val role: ChatRole,
    val text: String,
)

data class ChatMessage(
    val id: String,
    val seq: Long,
    val version: Long,
    val clientId: String?,
    val role: ChatRole,
    val kind: ChatKind,
    val text: String,
    val replyTo: ChatQuote?,
    val status: ChatStatus,
    val errorKey: String?,
    val createdAtMillis: Long,
    val updatedAtMillis: Long,
    val attachments: List<ChatAttachment> = emptyList(),
    val transcript: String? = null,
    val alert: ChatAlert? = null,
)

data class ChatPage(
    val messages: List<ChatMessage>,
    val hasMore: Boolean,
    val cursor: Long,
)

data class ChatSent(
    val message: ChatMessage,
    val reply: ChatMessage,
)

sealed interface ChatEvent {
    data class Upsert(val message: ChatMessage) : ChatEvent

    data class Delta(val id: String, val offset: Int, val text: String) : ChatEvent
}

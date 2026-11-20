package dev.proflyder.home.core.model

enum class ChatRole { USER, ASSISTANT, SYSTEM }

enum class ChatKind { TEXT, ALERT }

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

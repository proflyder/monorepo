package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ChatAttachment
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.model.ChatSessionPatch
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.Flow

data class ChatSendRequest(
    val sessionId: String,
    val clientId: String,
    val text: String,
    val replyTo: String?,
    val attachments: List<String> = emptyList(),
)

interface ChatClient {
    suspend fun page(sessionId: String, before: Long?, limit: Int): Outcome<ChatPage>

    suspend fun send(request: ChatSendRequest): Outcome<ChatSent>

    suspend fun upload(bytes: ByteArray, mime: String, name: String): Outcome<ChatAttachment>

    suspend fun download(id: String): Outcome<ByteArray>

    suspend fun retry(id: String): Outcome<ChatMessage>

    suspend fun stop(id: String): Outcome<ChatMessage>

    suspend fun sessions(archived: Boolean): Outcome<List<ChatSession>>

    suspend fun createSession(title: String?): Outcome<ChatSession>

    suspend fun patchSession(id: String, patch: ChatSessionPatch): Outcome<ChatSession>

    suspend fun deleteSession(id: String): Outcome<Unit>

    fun events(sessionId: String?, after: Long): Flow<ChatEvent>
}

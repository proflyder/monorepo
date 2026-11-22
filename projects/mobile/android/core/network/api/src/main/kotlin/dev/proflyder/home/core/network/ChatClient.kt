package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ChatAttachment
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.Flow

interface ChatClient {
    suspend fun page(before: Long?, limit: Int): Outcome<ChatPage>

    suspend fun send(clientId: String, text: String, replyTo: String?, attachments: List<String> = emptyList()): Outcome<ChatSent>

    suspend fun upload(bytes: ByteArray, mime: String, name: String): Outcome<ChatAttachment>

    suspend fun download(id: String): Outcome<ByteArray>

    suspend fun retry(id: String): Outcome<ChatMessage>

    fun events(after: Long): Flow<ChatEvent>
}

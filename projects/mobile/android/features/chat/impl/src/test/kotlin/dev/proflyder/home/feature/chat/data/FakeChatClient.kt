package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatAttachment
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.feature.chat.ChatFixtures
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

internal class FakeChatClient : ChatClient {
    var page: Outcome<ChatPage> = Outcome.Success(ChatPage(emptyList(), hasMore = false, cursor = 0))
    val sendResults = ArrayDeque<Outcome<ChatSent>>()
    val sentIds = mutableListOf<String>()
    var seq = 10L

    override suspend fun page(before: Long?, limit: Int): Outcome<ChatPage> = page

    val uploads = mutableListOf<String>()
    val sentAttachments = mutableListOf<List<String>>()
    val uploadResults = ArrayDeque<Outcome<ChatAttachment>>()

    override suspend fun upload(bytes: ByteArray, mime: String, name: String): Outcome<ChatAttachment> {
        uploads += name
        return uploadResults.removeFirstOrNull()
            ?: Outcome.Success(ChatAttachment("f_$name", ChatAttachmentKind.VOICE, mime, bytes.size.toLong(), name, 1000))
    }

    override suspend fun download(id: String): Outcome<ByteArray> = Outcome.Success(ByteArray(0))

    override suspend fun send(clientId: String, text: String, replyTo: String?, attachments: List<String>): Outcome<ChatSent> {
        sentIds += clientId
        sentAttachments += attachments
        return sendResults.removeFirstOrNull() ?: Outcome.Success(accepted(clientId, text))
    }

    override suspend fun retry(id: String): Outcome<ChatMessage> =
        Outcome.Success(ChatFixtures.message(id, seq, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING))

    var events: Flow<ChatEvent> = emptyFlow()
    var streams = 0

    override fun events(after: Long): Flow<ChatEvent> = events.also { streams++ }

    fun accepted(clientId: String, text: String): ChatSent {
        seq += 2
        return ChatSent(
            ChatFixtures.message("u$seq", seq, ChatRole.USER, text, clientId = clientId),
            ChatFixtures.message("a$seq", seq + 1, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING),
        )
    }
}

internal class MemoryOutboxStore(initial: List<OutgoingRecord> = emptyList()) : ChatOutboxStore {
    var records: List<OutgoingRecord> = initial

    override suspend fun load(): List<OutgoingRecord> = records

    override suspend fun save(records: List<OutgoingRecord>) {
        this.records = records
    }
}

package dev.proflyder.home.feature.chat.data

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

    override suspend fun send(clientId: String, text: String, replyTo: String?): Outcome<ChatSent> {
        sentIds += clientId
        return sendResults.removeFirstOrNull() ?: Outcome.Success(accepted(clientId, text))
    }

    override suspend fun retry(id: String): Outcome<ChatMessage> =
        Outcome.Success(ChatFixtures.message(id, seq, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING))

    override fun events(after: Long): Flow<ChatEvent> = emptyFlow()

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

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
import dev.proflyder.home.core.network.ChatSendRequest
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.model.ChatSessionPatch
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.chat.ChatFixtures
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

internal class FakeChatClient : ChatClient {
    var page: Outcome<ChatPage> = Outcome.Success(ChatPage(emptyList(), hasMore = false, cursor = 0))
    val sendResults = ArrayDeque<Outcome<ChatSent>>()
    val sentIds = mutableListOf<String>()
    var seq = 10L

    val pagedSessions = mutableListOf<String>()

    override suspend fun page(sessionId: String, before: Long?, limit: Int): Outcome<ChatPage> = page.also { pagedSessions += sessionId }

    val uploads = mutableListOf<String>()
    val sentAttachments = mutableListOf<List<String>>()
    val uploadResults = ArrayDeque<Outcome<ChatAttachment>>()

    override suspend fun upload(bytes: ByteArray, mime: String, name: String): Outcome<ChatAttachment> {
        uploads += name
        return uploadResults.removeFirstOrNull()
            ?: Outcome.Success(ChatAttachment("f_$name", ChatAttachmentKind.VOICE, mime, bytes.size.toLong(), name, 1000))
    }

    override suspend fun download(id: String): Outcome<ByteArray> = Outcome.Success(ByteArray(0))

    val sentSessions = mutableListOf<String>()

    override suspend fun send(request: ChatSendRequest): Outcome<ChatSent> {
        sentIds += request.clientId
        sentSessions += request.sessionId
        sentAttachments += request.attachments
        return sendResults.removeFirstOrNull() ?: Outcome.Success(accepted(request.clientId, request.text))
    }

    override suspend fun retry(id: String): Outcome<ChatMessage> =
        Outcome.Success(ChatFixtures.message(id, seq, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING))

    var events: Flow<ChatEvent> = emptyFlow()
    var streams = 0

    val streamedSessions = mutableListOf<String?>()

    override fun events(sessionId: String?, after: Long): Flow<ChatEvent> = events.also {
        streams++
        streamedSessions += sessionId
    }

    var sessions: MutableList<ChatSession> = mutableListOf()
    val stopped = mutableListOf<String>()
    val patches = mutableListOf<Pair<String, ChatSessionPatch>>()
    val deleted = mutableListOf<String>()
    var sessionFailure: RemoteFailure? = null

    override suspend fun stop(id: String): Outcome<ChatMessage> {
        stopped += id
        return Outcome.Success(ChatFixtures.message(id, STOPPED_SEQ, ChatRole.ASSISTANT, "Начал", status = ChatStatus.FAILED))
    }

    override suspend fun sessions(archived: Boolean): Outcome<List<ChatSession>> =
        sessionFailure?.let { Outcome.Failure(it) } ?: Outcome.Success(sessions.filter { it.archived == archived })

    override suspend fun createSession(title: String?): Outcome<ChatSession> {
        val created = ChatFixtures.session("s_new", title = title, updatedAt = ChatFixtures.NOON + 3_600_000)
        sessions += created
        return Outcome.Success(created)
    }

    override suspend fun patchSession(id: String, patch: ChatSessionPatch): Outcome<ChatSession> {
        patches += id to patch
        val current = sessions.first { it.id == id }
        val updated = current.copy(
            title = patch.title ?: current.title,
            pinned = patch.pinned ?: current.pinned,
            archived = patch.archived ?: current.archived,
        )
        sessions = sessions.map { if (it.id == id) updated else it }.toMutableList()
        return Outcome.Success(updated)
    }

    override suspend fun deleteSession(id: String): Outcome<Unit> {
        deleted += id
        sessions.removeAll { it.id == id }
        return Outcome.Success(Unit)
    }

    fun accepted(clientId: String, text: String): ChatSent {
        seq += 2
        return ChatSent(
            ChatFixtures.message("u$seq", seq, ChatRole.USER, text, clientId = clientId),
            ChatFixtures.message("a$seq", seq + 1, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING),
        )
    }
}

private const val STOPPED_SEQ = 99L

internal class MemoryOutboxStore(initial: List<OutgoingRecord> = emptyList()) : ChatOutboxStore {
    var records: List<OutgoingRecord> = initial

    override suspend fun load(): List<OutgoingRecord> = records

    override suspend fun save(records: List<OutgoingRecord>) {
        this.records = records
    }
}

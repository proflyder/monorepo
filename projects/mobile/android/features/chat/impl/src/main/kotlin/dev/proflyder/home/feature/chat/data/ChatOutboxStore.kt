package dev.proflyder.home.feature.chat.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.LocalJson
import dev.proflyder.home.core.operation.suspendRunCatching
import java.io.IOException
import kotlinx.coroutines.flow.first
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

internal interface ChatOutboxStore {
    suspend fun load(): List<OutgoingRecord>

    suspend fun save(records: List<OutgoingRecord>)
}

@Serializable
internal data class OutgoingRecord(
    val clientId: String = "",
    val text: String = "",
    val replyId: String? = null,
    val replyRole: String? = null,
    val replyText: String? = null,
    @SerialName("createdAt") val createdAtMillis: Long = 0,
    @SerialName("failure") val failureKey: String? = null,
    val attachments: List<AttachmentRecord> = emptyList(),
)

@Serializable
internal data class AttachmentRecord(
    val path: String = "",
    val kind: String = "",
    val mime: String = "",
    val name: String = "",
    val size: Long = 0,
    val durationMs: Long? = null,
    val uploadedId: String? = null,
)

internal fun Outgoing.toRecord(): OutgoingRecord = OutgoingRecord(
    clientId = clientId,
    text = text,
    replyId = replyTo?.id,
    replyRole = replyTo?.role?.name,
    replyText = replyTo?.text,
    createdAtMillis = createdAtMillis,
    failureKey = failureKey,
    attachments = attachments.map {
        AttachmentRecord(it.path, it.kind.name, it.mime, it.name, it.size, it.durationMs, it.uploadedId)
    },
)

internal fun OutgoingRecord.toOutgoing(): Outgoing = Outgoing(
    clientId = clientId,
    text = text,
    replyTo = replyId?.let { ChatQuote(it, roleOf(replyRole), replyText.orEmpty()) },
    createdAtMillis = createdAtMillis,
    failureKey = failureKey,
    attachments = attachments.map(AttachmentRecord::toLocal),
)

private fun AttachmentRecord.toLocal(): LocalAttachment = LocalAttachment(
    path = path,
    kind = ChatAttachmentKind.entries.firstOrNull { it.name == kind } ?: ChatAttachmentKind.FILE,
    mime = mime,
    name = name,
    size = size,
    durationMs = durationMs,
    uploadedId = uploadedId,
)

private fun roleOf(name: String?): ChatRole = ChatRole.entries.firstOrNull { it.name == name } ?: ChatRole.ASSISTANT

internal object OutboxCodec {
    private val serializer = ListSerializer(OutgoingRecord.serializer())

    fun encode(records: List<OutgoingRecord>): String = LocalJson.encodeToString(serializer, records)

    fun decode(raw: String?): List<OutgoingRecord> = raw
        ?.let { runCatching { LocalJson.decodeFromString(serializer, it) }.getOrNull() }
        .orEmpty()
        .filter { it.clientId.isNotEmpty() }
}

internal class DataStoreChatOutboxStore(
    private val dataStore: DataStore<Preferences>,
    sessionId: String,
) : ChatOutboxStore {
    private val key = outboxKey(sessionId)

    override suspend fun load(): List<OutgoingRecord> = suspendRunCatching {
        OutboxCodec.decode(dataStore.data.first()[key])
    }.getOrElse { emptyList() }

    override suspend fun save(records: List<OutgoingRecord>) {
        suspendRunCatching {
            dataStore.edit { it[key] = OutboxCodec.encode(records) }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    private companion object {
        const val LEGACY_KEY = "chat_outbox"

        fun outboxKey(sessionId: String) = stringPreferencesKey(
            if (sessionId == ChatSessions.LEGACY_ID) LEGACY_KEY else "${LEGACY_KEY}_$sessionId",
        )
    }
}

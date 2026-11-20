package dev.proflyder.home.feature.chat.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.first
import org.json.JSONArray
import org.json.JSONObject

interface ChatOutboxStore {
    suspend fun load(): List<OutgoingRecord>

    suspend fun save(records: List<OutgoingRecord>)
}

data class OutgoingRecord(
    val clientId: String,
    val text: String,
    val replyId: String?,
    val replyRole: String?,
    val replyText: String?,
    val createdAtMillis: Long,
    val failureKey: String?,
    val attachments: List<AttachmentRecord> = emptyList(),
)

data class AttachmentRecord(
    val path: String,
    val kind: String,
    val mime: String,
    val name: String,
    val size: Long,
    val durationMs: Long?,
    val uploadedId: String?,
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
    fun encode(records: List<OutgoingRecord>): String =
        JSONArray(records.map(::toJson)).toString()

    fun decode(raw: String?): List<OutgoingRecord> {
        val array = raw?.let { runCatching { JSONArray(it) }.getOrNull() } ?: return emptyList()
        return (0 until array.length()).mapNotNull { index -> array.optJSONObject(index)?.let(::fromJson) }
    }

    private fun toJson(record: OutgoingRecord): JSONObject = JSONObject()
        .put(CLIENT_ID, record.clientId)
        .put(TEXT, record.text)
        .put(REPLY_ID, record.replyId)
        .put(REPLY_ROLE, record.replyRole)
        .put(REPLY_TEXT, record.replyText)
        .put(CREATED, record.createdAtMillis)
        .put(FAILURE, record.failureKey)
        .put(ATTACHMENTS, JSONArray(record.attachments.map(::attachmentJson)))

    private fun attachmentJson(record: AttachmentRecord): JSONObject = JSONObject()
        .put(PATH, record.path)
        .put(KIND, record.kind)
        .put(MIME, record.mime)
        .put(NAME, record.name)
        .put(SIZE, record.size)
        .put(DURATION, record.durationMs)
        .put(UPLOADED, record.uploadedId)

    private fun attachmentOf(json: JSONObject): AttachmentRecord = AttachmentRecord(
        path = json.optString(PATH),
        kind = json.optString(KIND),
        mime = json.optString(MIME),
        name = json.optString(NAME),
        size = json.optLong(SIZE),
        durationMs = if (json.has(DURATION) && !json.isNull(DURATION)) json.optLong(DURATION) else null,
        uploadedId = json.optStringOrNull(UPLOADED),
    )

    private fun attachmentsOf(json: JSONObject): List<AttachmentRecord> {
        val array = json.optJSONArray(ATTACHMENTS) ?: return emptyList()
        return (0 until array.length()).mapNotNull { array.optJSONObject(it)?.let(::attachmentOf) }
    }

    private fun fromJson(json: JSONObject): OutgoingRecord? = json.optString(CLIENT_ID).takeIf { it.isNotEmpty() }?.let {
        OutgoingRecord(
            clientId = it,
            text = json.optString(TEXT),
            replyId = json.optStringOrNull(REPLY_ID),
            replyRole = json.optStringOrNull(REPLY_ROLE),
            replyText = json.optStringOrNull(REPLY_TEXT),
            createdAtMillis = json.optLong(CREATED),
            failureKey = json.optStringOrNull(FAILURE),
            attachments = attachmentsOf(json),
        )
    }

    private fun JSONObject.optStringOrNull(key: String): String? =
        if (has(key) && !isNull(key)) optString(key) else null

    private const val CLIENT_ID = "clientId"
    private const val TEXT = "text"
    private const val REPLY_ID = "replyId"
    private const val REPLY_ROLE = "replyRole"
    private const val REPLY_TEXT = "replyText"
    private const val CREATED = "createdAt"
    private const val FAILURE = "failure"
    private const val ATTACHMENTS = "attachments"
    private const val PATH = "path"
    private const val KIND = "kind"
    private const val MIME = "mime"
    private const val NAME = "name"
    private const val SIZE = "size"
    private const val DURATION = "durationMs"
    private const val UPLOADED = "uploadedId"
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreChatOutboxStore(
    private val dataStore: DataStore<Preferences>,
) : ChatOutboxStore {
    override suspend fun load(): List<OutgoingRecord> = suspendRunCatching {
        OutboxCodec.decode(dataStore.data.first()[KEY_OUTBOX])
    }.getOrElse { emptyList() }

    override suspend fun save(records: List<OutgoingRecord>) {
        suspendRunCatching {
            dataStore.edit { it[KEY_OUTBOX] = OutboxCodec.encode(records) }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    private companion object {
        val KEY_OUTBOX = stringPreferencesKey("chat_outbox")
    }
}

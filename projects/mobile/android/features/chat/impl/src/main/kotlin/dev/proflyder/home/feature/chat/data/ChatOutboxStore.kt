package dev.proflyder.home.feature.chat.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
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
)

internal fun Outgoing.toRecord(): OutgoingRecord = OutgoingRecord(
    clientId = clientId,
    text = text,
    replyId = replyTo?.id,
    replyRole = replyTo?.role?.name,
    replyText = replyTo?.text,
    createdAtMillis = createdAtMillis,
    failureKey = failureKey,
)

internal fun OutgoingRecord.toOutgoing(): Outgoing = Outgoing(
    clientId = clientId,
    text = text,
    replyTo = replyId?.let { ChatQuote(it, roleOf(replyRole), replyText.orEmpty()) },
    createdAtMillis = createdAtMillis,
    failureKey = failureKey,
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

    private fun fromJson(json: JSONObject): OutgoingRecord? = json.optString(CLIENT_ID).takeIf { it.isNotEmpty() }?.let {
        OutgoingRecord(
            clientId = it,
            text = json.optString(TEXT),
            replyId = json.optStringOrNull(REPLY_ID),
            replyRole = json.optStringOrNull(REPLY_ROLE),
            replyText = json.optStringOrNull(REPLY_TEXT),
            createdAtMillis = json.optLong(CREATED),
            failureKey = json.optStringOrNull(FAILURE),
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

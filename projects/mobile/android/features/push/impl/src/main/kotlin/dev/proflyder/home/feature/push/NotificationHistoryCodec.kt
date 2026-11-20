package dev.proflyder.home.feature.push

import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

@Serializable
internal class NotificationHistoryRecord(
    val id: String = "",
    val type: String = "",
    val title: String = "",
    val body: String = "",
    val deepLink: String? = null,
    val receivedAtMillis: Long = 0,
)

internal object NotificationHistoryCodec {
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }
    private val serializer = ListSerializer(NotificationHistoryRecord.serializer())

    fun encode(entries: List<NotificationHistoryEntry>): String =
        json.encodeToString(serializer, entries.map(::toRecord))

    fun decode(raw: String?): List<NotificationHistoryEntry> {
        if (raw.isNullOrBlank()) return emptyList()
        return runCatching { json.decodeFromString(serializer, raw) }
            .getOrElse { emptyList() }
            .mapNotNull(::toEntry)
            .let(NotificationHistoryRules::bounded)
    }

    private fun toRecord(entry: NotificationHistoryEntry): NotificationHistoryRecord =
        NotificationHistoryRecord(
            id = entry.id,
            type = entry.type.name,
            title = entry.title,
            body = entry.body,
            deepLink = entry.deepLink,
            receivedAtMillis = entry.receivedAtMillis,
        )

    private fun toEntry(record: NotificationHistoryRecord): NotificationHistoryEntry? {
        if (record.id.isBlank() || record.title.isBlank()) return null
        return NotificationHistoryEntry(
            id = record.id,
            type = typeOf(record.type),
            title = record.title,
            body = record.body,
            deepLink = record.deepLink,
            receivedAtMillis = record.receivedAtMillis,
        )
    }

    private fun typeOf(raw: String): NotificationHistoryType = when (raw) {
        NotificationHistoryType.FIND.name -> NotificationHistoryType.FIND
        else -> NotificationHistoryType.NOTIFY
    }
}

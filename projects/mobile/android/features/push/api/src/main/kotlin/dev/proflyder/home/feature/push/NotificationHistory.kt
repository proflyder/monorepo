package dev.proflyder.home.feature.push

import kotlinx.coroutines.flow.StateFlow

enum class NotificationHistoryType {
    NOTIFY,

    FIND,
}

data class NotificationHistoryEntry(
    val id: String,
    val type: NotificationHistoryType,
    val title: String,
    val body: String,
    val deepLink: String? = null,
    val receivedAtMillis: Long,
)

object NotificationHistoryRules {
    const val MAX_ENTRIES: Int = 200

    fun record(
        entries: List<NotificationHistoryEntry>,
        entry: NotificationHistoryEntry,
    ): List<NotificationHistoryEntry> =
        (listOf(entry) + entries.filterNot { it.id == entry.id }).take(MAX_ENTRIES)

    fun bounded(entries: List<NotificationHistoryEntry>): List<NotificationHistoryEntry> =
        entries.take(MAX_ENTRIES)
}

interface NotificationHistoryStore {
    val entries: StateFlow<List<NotificationHistoryEntry>>

    suspend fun load()

    suspend fun record(entry: NotificationHistoryEntry)

    suspend fun clear()
}

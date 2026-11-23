package dev.proflyder.home.core.model

enum class NotificationHistoryType {
    NOTIFY,
    FIND,
    ;

    companion object {
        fun parse(raw: String?): NotificationHistoryType = if (raw?.trim() == "find") FIND else NOTIFY
    }
}

data class NotificationHistory(
    val id: String,
    val type: NotificationHistoryType,
    val title: String,
    val body: String,
    val deepLink: String? = null,
    val createdUnix: Long,
    val read: Boolean = false,
)

data class NotificationHistoryPage(
    val entries: List<NotificationHistory>,
    val nextCursor: String?,
)

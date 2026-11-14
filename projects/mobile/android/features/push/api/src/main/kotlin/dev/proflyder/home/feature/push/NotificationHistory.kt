package dev.proflyder.home.feature.push

import kotlinx.coroutines.flow.StateFlow

/** Why the phone received a message; the history row picks its icon from it. */
enum class NotificationHistoryType {
    /** A `phone_notify` (or ordinary `notification`) message. */
    NOTIFY,

    /** A `phone_find` request that rang the phone. */
    FIND,
}

/**
 * One FCM message this phone actually received, as the history screen shows it.
 *
 * The text is the decoded payload, already bounded by [PushPayloads], and
 * [deepLink] is the validated `home://…` route (never an arbitrary URL). The
 * entry holds nothing secret: no token, no server address.
 */
data class NotificationHistoryEntry(
    /** Stable per-message id; used to keep one row when a message is re-recorded. */
    val id: String,
    val type: NotificationHistoryType,
    val title: String,
    val body: String,
    /** In-app route tapped from the row; `null` when the message carried none. */
    val deepLink: String? = null,
    /** Wall-clock time the phone received the message (unix millis). */
    val receivedAtMillis: Long,
)

/**
 * Retention rules, kept pure so they are unit-tested without Android: newest
 * first, a repeated [NotificationHistoryEntry.id] replaces its older copy, and
 * the log never grows past [MAX_ENTRIES].
 */
object NotificationHistoryRules {
    /** How many messages the phone keeps; older ones are dropped. */
    const val MAX_ENTRIES: Int = 200

    /** The log with [entry] on top of [entries], bounded by [MAX_ENTRIES]. */
    fun record(
        entries: List<NotificationHistoryEntry>,
        entry: NotificationHistoryEntry,
    ): List<NotificationHistoryEntry> =
        (listOf(entry) + entries.filterNot { it.id == entry.id }).take(MAX_ENTRIES)

    /** The bounded projection of an already ordered log. */
    fun bounded(entries: List<NotificationHistoryEntry>): List<NotificationHistoryEntry> =
        entries.take(MAX_ENTRIES)
}

/**
 * Local log of received messages. It is written by the FCM service (which runs
 * with the app killed) and read by the history screen, so it persists to disk
 * and never depends on the server being reachable.
 */
interface NotificationHistoryStore {
    /** The log, newest first; updated after every write. */
    val entries: StateFlow<List<NotificationHistoryEntry>>

    /** Loads the persisted log once, before the screen renders. */
    suspend fun load()

    /** Prepends [entry], dropping the oldest messages beyond the retention limit. */
    suspend fun record(entry: NotificationHistoryEntry)

    /** Removes every stored message. */
    suspend fun clear()
}

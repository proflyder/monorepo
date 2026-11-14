package dev.proflyder.home.feature.push

import android.content.Context
import dev.proflyder.home.feature.push.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.UUID

/**
 * Turns a decoded FCM message into a history row.
 *
 * The find-phone request has no server text, so its row uses the same localized
 * title and the reason the ringing screen shows; an ordinary notification keeps
 * the server title, body and validated in-app deep link.
 */
@SingleIn(AppScope::class)
@Inject
class NotificationHistoryRecorder(
    private val context: Context,
    private val store: NotificationHistoryStore,
) {
    suspend fun record(message: PushMessage) {
        val entry = when (message) {
            is PushMessage.Find -> findEntry(message.reason)
            is PushMessage.Notify -> notifyEntry(message)
        }
        store.record(entry)
    }

    private fun findEntry(reason: String): NotificationHistoryEntry = entry(
        type = NotificationHistoryType.FIND,
        title = context.getString(R.string.push_find_title),
        body = reason.ifBlank { context.getString(R.string.push_find_default_reason) },
        deepLink = null,
    )

    private fun notifyEntry(message: PushMessage.Notify): NotificationHistoryEntry = entry(
        type = NotificationHistoryType.NOTIFY,
        title = message.title,
        body = message.body,
        deepLink = message.deepLink?.route,
    )

    private fun entry(
        type: NotificationHistoryType,
        title: String,
        body: String,
        deepLink: String?,
    ): NotificationHistoryEntry = NotificationHistoryEntry(
        id = UUID.randomUUID().toString(),
        type = type,
        title = title,
        body = body,
        deepLink = deepLink,
        receivedAtMillis = System.currentTimeMillis(),
    )
}

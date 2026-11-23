package dev.proflyder.home.feature.push

import android.content.Context
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.feature.push.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.UUID

@SingleIn(AppScope::class)
@Inject
internal class NotificationHistoryRecorder(
    private val context: Context,
    private val history: FeatureScope<NotificationHistoryStore>,
) {
    suspend fun record(message: PushMessage) {
        val entry = when (message) {
            is PushMessage.Find -> findEntry(message.reason)
            is PushMessage.Notify -> notifyEntry(message)
        }
        history.acquire().use { it.graph.record(entry) }
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

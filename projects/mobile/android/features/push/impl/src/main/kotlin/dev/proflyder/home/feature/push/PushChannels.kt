package dev.proflyder.home.feature.push

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import dev.proflyder.home.feature.push.impl.R

object PushChannels {
    const val NOTIFICATIONS: String = "home_notifications"

    const val FIND: String = "home_phone_find_v2"

    const val LEGACY_FIND: String = "home_phone_find"

    fun ensure(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(notifications(context))
        manager.createNotificationChannel(find(context))
        manager.deleteNotificationChannel(LEGACY_FIND)
    }

    private fun notifications(context: Context): NotificationChannel =
        NotificationChannel(
            NOTIFICATIONS,
            context.getString(R.string.push_channel_notifications_name),
            NotificationManager.IMPORTANCE_DEFAULT,
        ).apply {
            description = context.getString(R.string.push_channel_notifications_description)
            enableVibration(false)
        }

    private fun find(context: Context): NotificationChannel =
        NotificationChannel(
            FIND,
            context.getString(R.string.push_channel_find_name),
            NotificationManager.IMPORTANCE_HIGH,
        ).apply {
            description = context.getString(R.string.push_channel_find_description)
            enableVibration(false)
            setSound(null, null)
            setBypassDnd(true)
            lockscreenVisibility = android.app.Notification.VISIBILITY_PUBLIC
        }
}

object Vibration {
    val PATTERN: LongArray = longArrayOf(0, 800, 400, 800, 400)
}

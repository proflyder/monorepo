package dev.proflyder.home.feature.push

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.media.AudioAttributes
import android.provider.Settings
import dev.proflyder.home.feature.push.impl.R

/**
 * The app's two notification channels.
 *
 * Channels are created once and their importance/sound cannot be raised later,
 * so `home_phone_find` is created as an alarm channel: maximum importance, the
 * alarm audio usage (the phone keeps ringing in silent mode) and
 * `bypassDnd` (Do Not Disturb is overridden only while the owner granted the
 * app notification-policy access).
 *
 * The platform API is used directly: minSdk is 26, so `NotificationChannel`
 * exists everywhere, and the compat builder cannot express `bypassDnd` or a
 * per-channel sound with an explicit audio usage.
 */
object PushChannels {
    /** Ordinary notifications: title, body, deep link. */
    const val NOTIFICATIONS: String = "home_notifications"

    /** Find-phone: bypasses DND and uses the alarm audio stream. */
    const val FIND: String = "home_phone_find"

    /** Creates or updates both channels; safe to call on every start. */
    fun ensure(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(notifications(context))
        manager.createNotificationChannel(find(context))
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
            enableVibration(true)
            vibrationPattern = Vibration.PATTERN
            setBypassDnd(true)
            setSound(
                Settings.System.DEFAULT_ALARM_ALERT_URI,
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_ALARM)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build(),
            )
            lockscreenVisibility = android.app.Notification.VISIBILITY_PUBLIC
        }
}

/** Vibration pattern of the ringing screen: strong, repeating, unmistakable. */
object Vibration {
    val PATTERN: LongArray = longArrayOf(0, 800, 400, 800, 400)
}

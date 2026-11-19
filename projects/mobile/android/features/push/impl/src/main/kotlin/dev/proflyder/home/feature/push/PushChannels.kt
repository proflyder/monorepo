package dev.proflyder.home.feature.push

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import dev.proflyder.home.feature.push.impl.R

/**
 * The app's two notification channels.
 *
 * Channels are created once and their properties cannot be changed later, so a
 * channel id must never be reused with different settings: Android keeps the
 * original sound and vibration of `home_phone_find`, which is why the silent
 * replacement is a new id, `home_phone_find_v2`, and the old channel is
 * deleted on every start.
 *
 * The find channel carries no sound and no vibration of its own. The ring has
 * exactly one source, the [FindPhoneAudio] `MediaPlayer` in the foreground
 * service; a channel sound would play the same melody a second time, with an
 * offset, and would repeat on every notification update. `bypassDnd` is kept so
 * the request still reaches the owner when Do Not Disturb is on (the owner has
 * granted notification-policy access).
 *
 * The platform API is used directly: minSdk is 26, so `NotificationChannel`
 * exists everywhere, and the compat builder cannot express `bypassDnd`.
 */
object PushChannels {
    /** Ordinary notifications: title, body, deep link. */
    const val NOTIFICATIONS: String = "home_notifications"

    /** Find-phone: silent, bypasses DND; the ring itself is the MediaPlayer. */
    const val FIND: String = "home_phone_find_v2"

    /** The old channel that had an alarm sound; deleted because it is immutable. */
    const val LEGACY_FIND: String = "home_phone_find"

    /** Creates both channels and drops the legacy find channel. Safe on every start. */
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

/** Vibration pattern of the ringing screen: strong, repeating, unmistakable. */
object Vibration {
    val PATTERN: LongArray = longArrayOf(0, 800, 400, 800, 400)
}

package dev.proflyder.home.feature.push

import android.Manifest
import android.annotation.SuppressLint
import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dev.proflyder.home.feature.push.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/**
 * Draws the two notification shapes. All user-visible text comes from
 * resources; a server message supplies the title and body of an ordinary
 * notification and nothing else.
 *
 * The find-phone notification is the loud one: the `home_phone_find` channel
 * (alarm usage, bypass-DND) with a full-screen intent that opens the ringing
 * screen even over the lock screen. On Android 14+ this only becomes a
 * full-screen intent once the owner granted the allowance; otherwise the
 * notification is still high-importance and the service still rings.
 */
@SingleIn(AppScope::class)
@Inject
class PushNotifier(private val context: Context) {

    /** Shows an ordinary notification; the deep link is an in-app route. */
    fun notify(message: PushMessage.Notify) {
        val content = openAppIntent(message.deepLink)
        val notification = NotificationCompat.Builder(context, PushChannels.NOTIFICATIONS)
            .setSmallIcon(R.drawable.push_ic_notification)
            .setContentTitle(message.title)
            .setContentText(message.body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message.body))
            .setContentIntent(content)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .build()
        post(NOTIFY_ID, notification)
    }

    /** The ongoing ringing notification with the full-screen intent. */
    @Suppress("MissingPermission") // only posted by the foreground service
    fun ring(reason: String): Notification {
        val full = findScreenIntent()
        return NotificationCompat.Builder(context, PushChannels.FIND)
            .setSmallIcon(R.drawable.push_ic_notification)
            .setContentTitle(context.getString(R.string.push_find_title))
            .setContentText(
                reason.ifBlank { context.getString(R.string.push_find_default_reason) },
            )
            .setContentIntent(full)
            .setFullScreenIntent(full, true)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setDeleteIntent(stopIntent())
            .addAction(
                0,
                context.getString(R.string.push_find_found),
                stopIntent(),
            )
            .build()
    }

    /** Removes the ringing notification (the service also drops its own). */
    fun cancelRing() {
        NotificationManagerCompat.from(context).cancel(FIND_ID)
    }

    /** Deep link `home://alarms` selects the alarms tab; unknown routes open the app. */
    private fun openAppIntent(link: PushDeepLink?): PendingIntent {
        val intent = Intent(context, mainActivityClass())
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (link != null) intent.putExtra(PushIntents.EXTRA_DEEP_LINK, link.route)
        return PendingIntent.getActivity(
            context,
            REQUEST_OPEN,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    private fun findScreenIntent(): PendingIntent {
        val intent = Intent(context, FindPhoneActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(
            context,
            REQUEST_FIND,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    private fun stopIntent(): PendingIntent {
        val intent = FindPhoneService.stopIntent(context)
        return PendingIntent.getService(
            context,
            REQUEST_STOP,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    // The POST_NOTIFICATIONS guard lives in canPostNotifications(); lint cannot
    // see through the helper, so the suppression is scoped to this call.
    @SuppressLint("MissingPermission")
    private fun post(id: Int, notification: Notification) {
        // POST_NOTIFICATIONS is a runtime permission on Android 13+; without it
        // the system drops the notification, so the request is skipped instead
        // of crashing with a SecurityException.
        if (!canPostNotifications()) return
        runCatching {
            NotificationManagerCompat.from(context).notify(id, notification)
        }
    }

    private fun canPostNotifications(): Boolean {
        val postAllowed = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            true
        }
        return postAllowed && NotificationManagerCompat.from(context).areNotificationsEnabled()
    }

    private fun mainActivityClass(): Class<*> =
        runCatching { Class.forName(MAIN_ACTIVITY) }.getOrElse { FindPhoneActivity::class.java }

    private companion object {
        const val NOTIFY_ID = 71
        const val FIND_ID = 72
        const val REQUEST_OPEN = 1
        const val REQUEST_FIND = 2
        const val REQUEST_STOP = 3

        /** The app shell; resolved by name so this module stays independent of `app`. */
        const val MAIN_ACTIVITY = "dev.proflyder.home.MainActivity"
    }

    /** The id the foreground service starts with; the same record is updated. */
    fun findNotificationId(): Int = FIND_ID
}

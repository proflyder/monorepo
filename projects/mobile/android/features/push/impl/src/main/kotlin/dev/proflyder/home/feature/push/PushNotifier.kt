package dev.proflyder.home.feature.push

import android.annotation.SuppressLint
import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import dev.proflyder.home.core.launchericon.ThemedNotifications
import dev.proflyder.home.core.ui.NotificationPermission
import dev.proflyder.home.feature.push.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@Inject
internal class PushNotifier(
    private val context: Context,
    private val themed: ThemedNotifications,
    private val permission: NotificationPermission,
) {
    fun notify(message: PushMessage.Notify) {
        val content = openAppIntent(message.deepLink)
        val builder = NotificationCompat.Builder(context, PushChannels.NOTIFICATIONS)
            .setSmallIcon(R.drawable.push_ic_notification)
            .setContentTitle(message.title)
            .setContentText(message.body)
        val notification = themed.conversation(builder, message.title, message.body)
            .setContentIntent(content)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .build()
        post(NOTIFY_ID, notification)
    }

    @Suppress("MissingPermission")
    fun ring(reason: String): Notification {
        val full = findScreenIntent()
        val builder = NotificationCompat.Builder(context, PushChannels.FIND)
        return themed.decorate(builder)
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

    fun cancelRing() {
        NotificationManagerCompat.from(context).cancel(FIND_ID)
    }

    private fun openAppIntent(link: PushDeepLink?): PendingIntent {
        val intent = launchIntent()
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

    @SuppressLint("MissingPermission")
    private fun post(id: Int, notification: Notification) {
        if (!permission.granted()) return
        runCatching {
            NotificationManagerCompat.from(context).notify(id, notification)
        }
    }

    private fun launchIntent(): Intent =
        context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?: Intent(context, FindPhoneActivity::class.java)

    private companion object {
        const val NOTIFY_ID = 71
        const val FIND_ID = 72
        const val REQUEST_OPEN = 1
        const val REQUEST_FIND = 2
        const val REQUEST_STOP = 3
    }

    fun findNotificationId(): Int = FIND_ID
}

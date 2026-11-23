package dev.proflyder.home.feature.vpn.tunnel

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.annotation.ColorInt
import androidx.core.app.NotificationCompat
import dev.proflyder.home.feature.push.PushIntents
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.presentation.VpnTexts

internal object VpnNotifications {
    const val CHANNEL_ID = "vpn_status"
    const val NOTIFICATION_ID = 4_210

    fun ensureChannel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val channel = NotificationChannel(
            CHANNEL_ID,
            context.getString(R.string.vpn_channel_name),
            NotificationManager.IMPORTANCE_LOW,
        )
        channel.setShowBadge(false)
        manager.createNotificationChannel(channel)
    }

    fun build(context: Context, connection: VpnConnection, fallback: Intent?, @ColorInt accent: Int): android.app.Notification {
        val builder = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.vpn_ic_shield)
            .setColor(accent)
            .setContentTitle(context.getString(VpnTexts.title(connection)))
            .setContentText(VpnTexts.detail(context, connection, System.currentTimeMillis()))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(openApp(context))
            .addAction(0, context.getString(R.string.vpn_disconnect), disconnect(context))
        fallback?.let { builder.addAction(0, context.getString(R.string.vpn_open_app), activity(context, it)) }
        (connection as? VpnConnection.Connected)?.let { builder.setWhen(it.sinceMillis).setShowWhen(true).setUsesChronometer(true) }
        return builder.build()
    }

    fun openAppIntent(context: Context): Intent? =
        context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?.putExtra(PushIntents.EXTRA_DEEP_LINK, VpnDestinationRoute.ROUTE)
            ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)

    private fun openApp(context: Context): PendingIntent? = openAppIntent(context)?.let { activity(context, it) }

    private fun activity(context: Context, intent: Intent): PendingIntent = PendingIntent.getActivity(
        context,
        intent.hashCode(),
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun disconnect(context: Context): PendingIntent = PendingIntent.getService(
        context,
        REQUEST_DISCONNECT,
        Intent(context, VpnStatusService::class.java).setAction(VpnStatusService.ACTION_DISCONNECT),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private const val REQUEST_DISCONNECT = 1
}

internal object VpnDestinationRoute {
    const val ROUTE = "vpn/main"
}

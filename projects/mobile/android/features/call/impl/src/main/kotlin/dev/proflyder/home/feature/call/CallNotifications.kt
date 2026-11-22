package dev.proflyder.home.feature.call

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.presentation.CallActivity
import dev.proflyder.home.feature.call.presentation.CallTexts

internal class CallNotifications(private val context: Context) {
    private val texts = CallTexts(context)

    fun createChannel() {
        val channel = NotificationChannelCompat.Builder(CHANNEL_ID, NotificationManagerCompat.IMPORTANCE_HIGH)
            .setName(context.getString(R.string.call_channel_name))
            .setDescription(context.getString(R.string.call_channel_description))
            .setShowBadge(false)
            .build()
        NotificationManagerCompat.from(context).createNotificationChannel(channel)
    }

    fun build(state: CallSnapshot): Notification {
        val station = context.getString(R.string.call_station_name)
        val person = Person.Builder().setName(station).setImportant(true).build()
        return NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.call_ic_phone)
            .setContentTitle(station)
            .setContentText(texts.phase(state.phase))
            .setContentIntent(contentIntent())
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setStyle(NotificationCompat.CallStyle.forOngoingCall(person, hangupIntent()))
            .build()
    }

    private fun hangupIntent(): PendingIntent = PendingIntent.getService(
        context,
        HANGUP_REQUEST,
        Intent(context, CallForegroundService::class.java).setAction(CallForegroundService.ACTION_HANGUP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun contentIntent(): PendingIntent = PendingIntent.getActivity(
        context,
        CONTENT_REQUEST,
        Intent(context, CallActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    companion object {
        const val NOTIFICATION_ID = 41
        private const val CHANNEL_ID = "home_calls"
        private const val HANGUP_REQUEST = 0
        private const val CONTENT_REQUEST = 1
    }
}

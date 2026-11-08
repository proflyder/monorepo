package dev.proflyder.home.feature.call

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import dev.proflyder.home.feature.call.impl.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.launch

/**
 * `phoneCall` foreground service with an ongoing CallStyle notification.
 *
 * Started when a call becomes live and stopped at IDLE/ENDED. "Сбросить" ends
 * the call through the same [CallController] the screen uses.
 */
class CallForegroundService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var observation: Job? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_HANGUP) {
            CallEntryPoint.require().controller.hangup()
            return START_NOT_STICKY
        }
        val controller = CallEntryPoint.require().controller
        post(controller.state.value)
        if (observation == null) {
            observation = scope.launch {
                controller.state.collect { state ->
                    if (state.isLive || state.phase == CallPhase.ENDED) {
                        post(state)
                    }
                    if (!state.isLive) {
                        stopForeground(STOP_FOREGROUND_REMOVE)
                        stopSelf()
                    }
                }
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        observation?.cancel()
        observation = null
        scope.coroutineContext.cancelChildren()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun post(state: CallSnapshot) {
        val notification = buildNotification(state)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun buildNotification(state: CallSnapshot): android.app.Notification {
        val person = Person.Builder().setName(getString(R.string.call_station_name)).setImportant(true).build()
        val hangup = hangupIntent()
        val content = contentIntent()
        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.call_ic_phone)
            .setContentTitle(getString(R.string.call_station_name))
            .setContentText(statusText(state))
            .setContentIntent(content)
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setStyle(NotificationCompat.CallStyle.forOngoingCall(person, hangup))
        return builder.build()
    }

    private fun statusText(state: CallSnapshot): String = when (state.phase) {
        CallPhase.IDLE, CallPhase.DIALING -> getString(R.string.call_status_dialing)
        CallPhase.RINGING -> getString(R.string.call_status_ringing)
        CallPhase.CONNECTING -> getString(R.string.call_status_connecting)
        CallPhase.ACTIVE -> getString(R.string.call_status_active)
        CallPhase.ENDED -> getString(R.string.call_status_ended)
    }

    private fun hangupIntent() = android.app.PendingIntent.getService(
        this,
        0,
        Intent(this, CallForegroundService::class.java).setAction(ACTION_HANGUP),
        android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun contentIntent() = android.app.PendingIntent.getActivity(
        this,
        1,
        Intent(this, dev.proflyder.home.feature.call.presentation.CallActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun createChannel() {
        val manager = NotificationManagerCompat.from(this)
        manager.createNotificationChannel(
            NotificationChannelCompat.Builder(CHANNEL_ID, NotificationManagerCompat.IMPORTANCE_HIGH)
                .setName(getString(R.string.call_channel_name))
                .setDescription(getString(R.string.call_channel_description))
                .setShowBadge(false)
                .build(),
        )
    }

    companion object {
        const val ACTION_HANGUP = "dev.proflyder.home.call.HANGUP"
        private const val CHANNEL_ID = "home_calls"
        private const val NOTIFICATION_ID = 41
    }
}

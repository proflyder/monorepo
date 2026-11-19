package dev.proflyder.home.feature.push

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

/**
 * Foreground service that keeps the ringing process alive.
 *
 * Started by [FindPhoneController] from a **high-priority FCM data message**,
 * which is exactly the case Android allows a background app to start a
 * foreground service from. It has no logic of its own: it plays the alarm
 * ([FindPhoneAudio]), posts the silent notification ([PushNotifier.ring]) and
 * stops when the controller says so, on «Нашёл» or after the 60-second timeout.
 *
 * There is exactly one sound source: this service's [FindPhoneAudio]. Repeated
 * starts and refreshes update the reason of the same notification and never
 * create a second `MediaPlayer`; [stopRinging] always stops the player,
 * cancels vibration and restores the alarm volume, including on `onDestroy`.
 *
 * The service type is `mediaPlayback`: the ring is a looping audio stream, and
 * the type requires no extra runtime permission beyond the declared
 * `FOREGROUND_SERVICE_MEDIA_PLAYBACK`.
 */
class FindPhoneService : Service() {
    private lateinit var audio: FindPhoneAudio
    private lateinit var notifier: PushNotifier
    private var ringing = false

    override fun onCreate() {
        super.onCreate()
        val dependencies = PushEntryPoint.require()
        audio = FindPhoneAudio(this)
        notifier = dependencies.notifications
        PushChannels.ensure(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> stopRinging()
            else -> startRinging(intent?.getStringExtra(EXTRA_REASON).orEmpty())
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        stopRinging()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun startRinging(reason: String) {
        if (!ringing) {
            ringing = true
            audio.start()
        }
        val notification = notifier.ring(reason)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                notifier.findNotificationId(),
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
            )
        } else {
            startForeground(notifier.findNotificationId(), notification)
        }
    }

    private fun stopRinging() {
        val wasRinging = ringing
        ringing = false
        audio.stop()
        notifier.cancelRing()
        if (wasRinging) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        }
        stopSelf()
    }

    companion object {
        const val ACTION_START: String = "dev.proflyder.home.push.FIND_START"
        const val ACTION_STOP: String = "dev.proflyder.home.push.FIND_STOP"
        const val ACTION_REFRESH: String = "dev.proflyder.home.push.FIND_REFRESH"
        const val EXTRA_REASON: String = "dev.proflyder.home.push.extra.REASON"

        /** Start intent for [FindPhoneController]. */
        fun startIntent(context: Context, reason: String): Intent =
            Intent(context, FindPhoneService::class.java)
                .setAction(ACTION_START)
                .putExtra(EXTRA_REASON, reason)

        /** Updates the reason of an already ringing service. */
        fun refresh(context: Context, reason: String) {
            runCatching {
                context.startService(
                    Intent(context, FindPhoneService::class.java)
                        .setAction(ACTION_REFRESH)
                        .putExtra(EXTRA_REASON, reason),
                )
            }
        }

        /** Stop intent for the controller and the notification action. */
        fun stopIntent(context: Context): Intent =
            Intent(context, FindPhoneService::class.java).setAction(ACTION_STOP)
    }
}

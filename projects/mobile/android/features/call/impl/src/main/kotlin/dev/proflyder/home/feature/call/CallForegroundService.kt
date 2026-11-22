package dev.proflyder.home.feature.call

import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

internal class CallForegroundService : LifecycleService() {

    private val controller: CallController by lazy { CallEntryPoint.require().controller }
    private val notifications by lazy { CallNotifications(this) }
    private var observation: Job? = null

    override fun onCreate() {
        super.onCreate()
        notifications.createChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_HANGUP -> controller.hangup()
            else -> observe()
        }
        return START_NOT_STICKY
    }

    private fun observe() {
        post(controller.state.value)
        if (observation != null) return
        observation = lifecycleScope.launch { controller.state.collect(::onState) }
    }

    private fun onState(state: CallSnapshot) {
        when (state.phase) {
            CallPhase.DIALING, CallPhase.RINGING, CallPhase.CONNECTING, CallPhase.ACTIVE -> post(state)
            CallPhase.ENDED -> stop(state)
            CallPhase.IDLE -> stop(null)
        }
    }

    private fun stop(last: CallSnapshot?) {
        last?.let(::post)
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun post(state: CallSnapshot) {
        val notification = notifications.build(state)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(CallNotifications.NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL)
        } else {
            startForeground(CallNotifications.NOTIFICATION_ID, notification)
        }
    }

    companion object {
        const val ACTION_HANGUP = "dev.proflyder.home.call.HANGUP"
    }
}

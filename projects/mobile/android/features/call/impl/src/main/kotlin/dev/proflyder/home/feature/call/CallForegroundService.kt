package dev.proflyder.home.feature.call

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import dev.proflyder.home.core.runtime.appGraph
import dev.zacsweers.metro.Inject
import dev.proflyder.home.core.launchericon.ThemedNotifications
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

class CallForegroundService : LifecycleService() {

    @Inject
    lateinit var controller: CallController

    @Inject
    lateinit var themed: ThemedNotifications
    private val notifications by lazy { CallNotifications(this, themed) }
    private var observation: Job? = null

    override fun onCreate() {
        appGraph<CallInjector>().inject(this)
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
            startForeground(CallNotifications.NOTIFICATION_ID, notification, callServiceTypes(this))
        } else {
            startForeground(CallNotifications.NOTIFICATION_ID, notification)
        }
    }

    companion object {
        internal fun callServiceTypes(context: Context): Int =
            callServiceTypes(Build.VERSION.SDK_INT, microphoneGranted(context))

        @SuppressLint("InlinedApi")
        internal fun callServiceTypes(sdk: Int, microphoneGranted: Boolean): Int {
            val microphone = sdk >= Build.VERSION_CODES.R && microphoneGranted
            val extra = if (microphone) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
            return ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL or extra
        }

        private fun microphoneGranted(context: Context): Boolean =
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) ==
                PackageManager.PERMISSION_GRANTED

        const val ACTION_HANGUP = "dev.proflyder.home.call.HANGUP"
    }
}

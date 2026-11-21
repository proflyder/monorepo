package dev.proflyder.home.feature.vpn.tunnel

import android.Manifest
import android.annotation.SuppressLint
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnEntryPoint
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class VpnStatusService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var observing = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val dependencies = VpnEntryPoint.require()
        VpnNotifications.ensureChannel(this)
        promote(dependencies.controller.connection.value)
        when (intent?.action) {
            ACTION_DISCONNECT -> dependencies.controller.disconnect()
            else -> observe()
        }
        return START_NOT_STICKY
    }

    private fun observe() {
        if (observing) return
        observing = true
        scope.launch { VpnEntryPoint.require().controller.connection.collect(::render) }
    }

    private fun render(connection: VpnConnection) {
        when (connection) {
            VpnConnection.Disconnected, is VpnConnection.Failed -> finish()
            else -> notify(connection)
        }
    }

    @SuppressLint("MissingPermission")
    private fun notify(connection: VpnConnection) {
        val manager = NotificationManagerCompat.from(this)
        when (canNotify()) {
            true -> runCatching { manager.notify(VpnNotifications.NOTIFICATION_ID, notification(connection)) }
            false -> Unit
        }
    }

    private fun promote(connection: VpnConnection) {
        ServiceCompat.startForeground(
            this,
            VpnNotifications.NOTIFICATION_ID,
            notification(connection),
            foregroundType(),
        )
    }

    private fun foregroundType(): Int = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        else -> 0
    }

    private fun canNotify(): Boolean = when {
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU -> true
        else -> checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    }

    private fun notification(connection: VpnConnection) = VpnNotifications.build(
        this,
        connection,
        fallbackIntent(connection),
    )

    private fun fallbackIntent(connection: VpnConnection): Intent? = when (connection) {
        VpnConnection.FallbackRequired -> VpnNotifications.openAppIntent(this)
        else -> null
    }

    private fun finish() {
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    companion object {
        const val ACTION_DISCONNECT = "dev.proflyder.home.vpn.DISCONNECT"

        fun start(context: Context) {
            val intent = Intent(context, VpnStatusService::class.java)
            runCatching { ContextCompat.startForegroundService(context, intent) }
        }
    }
}

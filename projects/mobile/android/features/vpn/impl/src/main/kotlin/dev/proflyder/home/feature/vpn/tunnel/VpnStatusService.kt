package dev.proflyder.home.feature.vpn.tunnel

import android.annotation.SuppressLint
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import dev.proflyder.home.core.runtime.appGraph
import dev.proflyder.home.core.launchericon.ThemedNotifications
import dev.proflyder.home.core.ui.NotificationPermission
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnInjector
import dev.proflyder.home.feature.vpn.connect.UserVpnCommands
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class VpnStatusService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var observing = false

    @Inject
    internal lateinit var controller: VpnController

    @Inject
    internal lateinit var commands: UserVpnCommands

    @Inject
    internal lateinit var notificationPermission: NotificationPermission

    @Inject
    internal lateinit var themed: ThemedNotifications

    override fun onCreate() {
        super.onCreate()
        appGraph<VpnInjector>().inject(this)
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        VpnNotifications.ensureChannel(this)
        promote(controller.connection.value)
        when (intent?.action) {
            ACTION_DISCONNECT -> commands.disconnect()
            else -> observe()
        }
        return START_NOT_STICKY
    }

    private fun observe() {
        if (observing) return
        observing = true
        scope.launch { controller.connection.collect(::render) }
    }

    private fun render(connection: VpnConnection) {
        when (connection) {
            VpnConnection.Disconnected, VpnConnection.NotNeededAtHome, is VpnConnection.Failed -> finish()
            else -> notify(connection)
        }
    }

    @SuppressLint("MissingPermission")
    private fun notify(connection: VpnConnection) {
        val manager = NotificationManagerCompat.from(this)
        when (notificationPermission.granted()) {
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

    private fun notification(connection: VpnConnection) = VpnNotifications.build(
        this,
        connection,
        fallbackIntent(connection),
        themed,
    )

    private fun fallbackIntent(connection: VpnConnection): Intent? = when (connection) {
        is VpnConnection.FallbackRequired -> VpnNotifications.openAppIntent(this)
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

        internal fun start(context: Context) {
            val intent = Intent(context, VpnStatusService::class.java)
            runCatching { ContextCompat.startForegroundService(context, intent) }
        }
    }
}

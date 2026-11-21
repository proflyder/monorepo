package dev.proflyder.home.feature.vpn.tunnel

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnEntryPoint
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.feature.vpn.presentation.VpnTexts
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

class VpnTileService : TileService() {
    private val root = SupervisorJob()
    private val scope = CoroutineScope(root + Dispatchers.Main.immediate)
    private var job: Job? = null

    override fun onStartListening() {
        super.onStartListening()
        job?.cancel()
        job = scope.launch { VpnEntryPoint.require().controller.connection.collect(::render) }
    }

    override fun onStopListening() {
        job?.cancel()
        super.onStopListening()
    }

    override fun onClick() {
        super.onClick()
        val controller = VpnEntryPoint.require().controller
        when {
            controller.connection.value.isActive -> controller.disconnect()
            controller.permissionIntent() != null -> openApp()
            else -> controller.connect()
        }
    }

    private fun render(connection: VpnConnection) {
        val tile = qsTile ?: return
        tile.state = when {
            connection.isActive -> Tile.STATE_ACTIVE
            else -> Tile.STATE_INACTIVE
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) tile.subtitle = getString(VpnTexts.title(connection))
        tile.updateTile()
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    private fun openApp() {
        val intent = VpnNotifications.openAppIntent(this) ?: return
        when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> startActivityAndCollapse(pending(intent))
            else -> startLegacy(intent)
        }
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    @Suppress("DEPRECATION")
    private fun startLegacy(intent: Intent) = startActivityAndCollapse(intent)

    private fun pending(intent: Intent): PendingIntent =
        PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    override fun onDestroy() {
        root.cancel()
        super.onDestroy()
    }
}

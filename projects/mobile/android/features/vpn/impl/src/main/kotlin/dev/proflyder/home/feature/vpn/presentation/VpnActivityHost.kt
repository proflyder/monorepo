package dev.proflyder.home.feature.vpn.presentation

import android.app.Activity
import android.view.View
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.feature.vpn.HomeRoute
import dev.proflyder.home.feature.vpn.tunnel.TunnelVpnController
import dev.proflyder.home.feature.vpn.tunnel.VpnOpenConnector
import dev.zacsweers.metro.Assisted
import dev.zacsweers.metro.AssistedFactory
import dev.zacsweers.metro.AssistedInject
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.core.ui.setVisible
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

@AssistedInject
class VpnActivityHost(
    @Assisted activity: ComponentActivity,
    @Assisted private val openScreen: () -> Unit,
    private val controller: TunnelVpnController,
    private val opener: VpnOpenConnector,
) {
    @AssistedFactory
    fun interface Factory {
        fun create(activity: ComponentActivity, openScreen: () -> Unit): VpnActivityHost
    }

    private val permission = activity.registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
        ::onPermission,
    )

    fun bind(button: MaterialButton) {
        button.setOnClickListener { toggle() }
        button.setOnLongClickListener {
            openScreen()
            true
        }
    }

    fun observe(scope: CoroutineScope, button: MaterialButton, banner: View) {
        scope.launch { controller.connection.collect { VpnToolbarToggle.render(button, it) } }
        scope.launch { opener.route.collect { banner.setVisible(it == HomeRoute.CONNECTING) } }
        scope.launch { opener.permissionRequests.collect { ask() } }
    }

    private fun ask() {
        when (val request = controller.permissionIntent()) {
            null -> opener.connect()
            else -> permission.launch(request)
        }
    }

    private fun toggle() {
        val request = controller.permissionIntent()
        when {
            controller.connection.value.isActive -> controller.disconnect()
            request != null -> permission.launch(request)
            else -> opener.connect()
        }
    }

    private fun onPermission(result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> opener.connect()
            else -> opener.release()
        }
    }
}

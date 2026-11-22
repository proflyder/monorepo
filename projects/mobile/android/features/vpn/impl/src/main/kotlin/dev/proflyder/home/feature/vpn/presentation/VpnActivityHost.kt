package dev.proflyder.home.feature.vpn.presentation

import android.app.Activity
import android.view.View
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.connect.AutoConnectCoordinator
import dev.proflyder.home.feature.vpn.connect.HomeRoute
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.feature.vpn.tunnel.VpnPermission
import dev.zacsweers.metro.Assisted
import dev.zacsweers.metro.AssistedFactory
import dev.zacsweers.metro.AssistedInject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

@AssistedInject
class VpnActivityHost internal constructor(
    @Assisted activity: ComponentActivity,
    @Assisted private val openScreen: () -> Unit,
    private val controller: VpnController,
    private val permission: VpnPermission,
    private val coordinator: AutoConnectCoordinator,
) {
    @AssistedFactory
    fun interface Factory {
        fun create(activity: ComponentActivity, openScreen: () -> Unit): VpnActivityHost
    }

    private val launcher = activity.registerForActivityResult(
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
        scope.launch { coordinator.route.collect { banner.setVisible(it == HomeRoute.CONNECTING) } }
        scope.launch { coordinator.permissionRequests.collect { askOrConnect() } }
    }

    private fun askOrConnect() {
        when (val request = permission.request()) {
            null -> coordinator.connectNow()
            else -> launcher.launch(request)
        }
    }

    private fun toggle() {
        when (controller.connection.value.isActive) {
            true -> controller.disconnect()
            false -> askOrConnect()
        }
    }

    private fun onPermission(result: ActivityResult) {
        coordinator.permissionResult(result.resultCode == Activity.RESULT_OK)
    }
}

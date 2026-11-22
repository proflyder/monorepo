package dev.proflyder.home.feature.vpn.presentation

import android.app.Activity
import android.view.View
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.feature.vpn.HomeRoute
import dev.proflyder.home.feature.vpn.VpnEntryPoint
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.core.ui.setVisible
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

class VpnActivityHost(
    private val activity: ComponentActivity,
    private val openScreen: () -> Unit,
) {
    private val dependencies get() = VpnEntryPoint.require()

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
        scope.launch { dependencies.controller.connection.collect { VpnToolbarToggle.render(button, it) } }
        scope.launch { dependencies.opener.route.collect { banner.setVisible(it == HomeRoute.CONNECTING) } }
        scope.launch { dependencies.opener.permissionRequests.collect { ask() } }
    }

    private fun ask() {
        when (val request = dependencies.controller.permissionIntent()) {
            null -> dependencies.opener.connect()
            else -> permission.launch(request)
        }
    }

    private fun toggle() {
        val controller = dependencies.controller
        val request = controller.permissionIntent()
        when {
            controller.connection.value.isActive -> controller.disconnect()
            request != null -> permission.launch(request)
            else -> dependencies.opener.connect()
        }
    }

    private fun onPermission(result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> dependencies.opener.connect()
            else -> dependencies.opener.release()
        }
    }
}

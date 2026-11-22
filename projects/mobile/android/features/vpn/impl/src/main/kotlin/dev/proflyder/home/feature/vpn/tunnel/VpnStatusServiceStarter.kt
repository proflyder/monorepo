package dev.proflyder.home.feature.vpn.tunnel

import android.content.Context
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.isActive
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

@Inject
internal class VpnStatusServiceStarter(
    private val context: Context,
    private val controller: VpnController,
    private val scope: CoroutineScope,
) : AppStartable {
    override fun start() {
        scope.launch {
            controller.connection.map { it.isActive }.distinctUntilChanged().filter { it }.collect {
                VpnStatusService.start(context)
            }
        }
    }
}

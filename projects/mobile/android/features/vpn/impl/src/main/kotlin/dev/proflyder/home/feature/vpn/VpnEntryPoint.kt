package dev.proflyder.home.feature.vpn

import dev.proflyder.home.feature.vpn.tunnel.HomeNetworkMonitor
import dev.proflyder.home.feature.vpn.tunnel.TunnelVpnController

object VpnEntryPoint {
    private var dependencies: Dependencies? = null

    class Dependencies(
        val controller: TunnelVpnController,
        val profiles: VpnProfiles,
        val settings: VpnSettingsStore,
        val homeNetwork: HomeNetworkMonitor,
    )

    fun install(value: Dependencies) {
        dependencies = value
        value.controller.start()
        value.homeNetwork.start()
    }

    fun require(): Dependencies =
        checkNotNull(dependencies) { "VpnEntryPoint was not installed by the application" }
}

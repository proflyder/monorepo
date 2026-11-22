package dev.proflyder.home.feature.vpn

import dev.proflyder.home.feature.vpn.tunnel.HomeNetworkMonitor
import dev.proflyder.home.feature.vpn.tunnel.TunnelVpnController
import dev.proflyder.home.feature.vpn.tunnel.VpnOpenConnector

object VpnEntryPoint {
    private var dependencies: Dependencies? = null

    class Dependencies(
        val controller: TunnelVpnController,
        val profiles: VpnProfiles,
        val settings: VpnSettingsStore,
        val homeNetwork: HomeNetworkMonitor,
        val opener: VpnOpenConnector,
    )

    fun install(value: Dependencies) {
        dependencies = value
        value.controller.start()
        value.homeNetwork.start()
        value.opener.start()
    }

    fun require(): Dependencies =
        checkNotNull(dependencies) { "VpnEntryPoint was not installed by the application" }
}

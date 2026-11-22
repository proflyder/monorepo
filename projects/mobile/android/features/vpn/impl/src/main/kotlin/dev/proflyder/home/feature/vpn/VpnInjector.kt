package dev.proflyder.home.feature.vpn

import dev.proflyder.home.feature.vpn.presentation.VpnActivityHost
import dev.proflyder.home.feature.vpn.tunnel.VpnStatusService
import dev.proflyder.home.feature.vpn.tunnel.VpnTileService
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesTo

@ContributesTo(AppScope::class)
interface VpnInjector {
    fun inject(service: VpnTileService)

    fun inject(service: VpnStatusService)

    val vpnActivityHostFactory: VpnActivityHost.Factory
}

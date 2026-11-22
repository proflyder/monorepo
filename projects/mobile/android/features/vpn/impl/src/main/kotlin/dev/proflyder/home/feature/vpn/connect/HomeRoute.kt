package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.core.network.RouteCondition
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.tunnel.TunnelSwitch
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow

internal enum class HomeRoute {
    READY,
    CHECKING,
    CONNECTING,
}

internal fun routeOpen(route: HomeRoute, connection: VpnConnection, switching: Boolean): Boolean =
    route == HomeRoute.READY && connection !is VpnConnection.Connecting && !switching

@Inject
internal class VpnRouteCondition(
    private val coordinator: Lazy<AutoConnectCoordinator>,
    controller: VpnController,
    tunnel: TunnelSwitch,
) : RouteCondition {
    override val ready: Flow<Boolean> =
        combine(flow { emitAll(coordinator.value.route) }, controller.connection, tunnel.switching, ::routeOpen)
            .distinctUntilChanged()
}

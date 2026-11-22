package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.core.network.RouteCondition
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map

internal enum class HomeRoute {
    READY,
    CHECKING,
    CONNECTING,
}

@Inject
internal class VpnRouteCondition(private val coordinator: Lazy<AutoConnectCoordinator>) : RouteCondition {
    override val ready: Flow<Boolean> = flow { emitAll(coordinator.value.route) }.map { it == HomeRoute.READY }
}

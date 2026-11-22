package dev.proflyder.home.feature.vpn.tunnel

import dev.proflyder.home.core.network.RouteCondition
import dev.proflyder.home.feature.vpn.HomeRoute
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map

@ContributesIntoSet(AppScope::class, binding = binding<RouteCondition>())
@Inject
class VpnRouteCondition(private val opener: Lazy<VpnOpenConnector>) : RouteCondition {
    override val ready: Flow<Boolean> = flow { emitAll(opener.value.route) }.map { it == HomeRoute.READY }
}

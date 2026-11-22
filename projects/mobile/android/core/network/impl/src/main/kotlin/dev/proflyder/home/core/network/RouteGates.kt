package dev.proflyder.home.core.network

import dev.proflyder.home.core.runtime.DefaultNetwork
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withTimeoutOrNull

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConditionRouteGate(
    conditions: Set<RouteCondition>,
) : RouteGate {
    private val timeout: Duration = ROUTE_WAIT
    private val ready: Flow<Boolean> = when {
        conditions.isEmpty() -> flowOf(true)
        else -> combine(conditions.map { it.ready }) { values -> values.all { it } }
    }

    override suspend fun await() {
        withTimeoutOrNull(timeout) { ready.first { it } }
    }

    companion object {
        val ROUTE_WAIT: Duration = 20.seconds
    }
}

@ContributesIntoSet(AppScope::class, binding = binding<RouteCondition>())
@Inject
class ActiveNetworkCondition(network: DefaultNetwork) : RouteCondition {
    override val ready: Flow<Boolean> = network.handle.map { it != null }
}

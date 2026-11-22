package dev.proflyder.home.core.network

import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.DefaultNetwork
import dev.proflyder.home.core.runtime.ForegroundState
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold
import okhttp3.OkHttpClient

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class ConnectionHealth(
    foreground: AppForeground,
    network: DefaultNetwork,
    conditions: Set<RouteCondition>,
    private val client: OkHttpClient,
    private val scope: CoroutineScope,
) : AppStartable {
    private val signals = staleConnectionSignals(foreground.state, network.handle, routeReady(conditions))

    override fun start() {
        signals.onEach { client.connectionPool.evictAll() }.launchIn(scope)
    }
}

internal val STALE_CONNECTION_AFTER: Duration = 15.seconds

internal fun staleConnectionSignals(
    foreground: Flow<ForegroundState>,
    network: Flow<Long?>,
    route: Flow<Boolean> = flowOf(true),
    threshold: Duration = STALE_CONNECTION_AFTER,
): Flow<Unit> = merge(foreground.staleReturns(threshold), network.drop(1).map { }, route.reopenings())

internal fun Flow<Boolean>.reopenings(): Flow<Unit> =
    distinctUntilChanged().drop(1).filter { it }.map { }

internal fun Flow<ForegroundState>.staleReturns(threshold: Duration): Flow<Unit> =
    runningFold(ForegroundTransition()) { transition, state -> transition.next(state) }
        .filter { it.isStaleReturn(threshold) }
        .map { }

internal data class ForegroundTransition(
    val previous: ForegroundState? = null,
    val current: ForegroundState? = null,
) {
    fun next(state: ForegroundState): ForegroundTransition = ForegroundTransition(current, state)

    fun isStaleReturn(threshold: Duration): Boolean {
        val from = previous ?: return false
        val to = current ?: return false
        return !from.foreground && to.foreground &&
            to.sinceElapsedNanos - from.sinceElapsedNanos >= threshold.inWholeNanoseconds
    }
}

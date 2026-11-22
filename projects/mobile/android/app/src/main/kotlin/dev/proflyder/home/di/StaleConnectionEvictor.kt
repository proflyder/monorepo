package dev.proflyder.home.di

import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.ForegroundState
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold
import okhttp3.OkHttpClient

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
internal class StaleConnectionEvictor(
    private val foreground: AppForeground,
    private val httpClient: OkHttpClient,
    private val scope: CoroutineScope,
) : AppStartable {
    override fun start() {
        foreground.state
            .runningFold(StaleTransition()) { previous, next -> previous.advance(next) }
            .filter { it.stale }
            .onEach { httpClient.connectionPool.evictAll() }
            .launchIn(scope)
    }
}

internal data class StaleTransition(
    val last: ForegroundState? = null,
    val stale: Boolean = false,
) {
    fun advance(next: ForegroundState): StaleTransition {
        val previous = last
        val returned = previous != null && !previous.foreground && next.foreground
        val away = next.sinceElapsedNanos - (previous?.sinceElapsedNanos ?: next.sinceElapsedNanos)
        return StaleTransition(next, returned && away >= STALE_CONNECTION_NANOS)
    }

    private companion object {
        const val STALE_CONNECTION_NANOS = 15_000_000_000L
    }
}

package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.tickerFlow
import kotlin.time.Duration
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge

internal class RefreshTriggers(foreground: AppForeground, period: Duration) {
    private val requests = MutableSharedFlow<Unit>(
        extraBufferCapacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    @OptIn(ExperimentalCoroutinesApi::class)
    private val whileForeground: Flow<Unit> = foreground.state
        .map { it.foreground }
        .distinctUntilChanged()
        .flatMapLatest { visible -> if (visible) tickerFlow(period) else emptyFlow() }

    val ticks: Flow<Unit> = merge(whileForeground, requests).conflate()

    fun request() {
        requests.tryEmit(Unit)
    }
}

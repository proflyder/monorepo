package dev.proflyder.home.core.runtime

import kotlin.time.Duration
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.isActive

fun tickerFlow(period: Duration, initialDelay: Duration = Duration.ZERO): Flow<Unit> {
    require(period.isPositive()) { "ticker period must be positive" }
    return flow {
        delay(initialDelay)
        while (currentCoroutineContext().isActive) {
            emit(Unit)
            delay(period)
        }
    }
}

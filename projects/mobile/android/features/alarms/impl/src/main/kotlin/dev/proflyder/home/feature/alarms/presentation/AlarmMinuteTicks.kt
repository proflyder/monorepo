package dev.proflyder.home.feature.alarms.presentation

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

private const val MILLIS_PER_MINUTE = 60_000L

internal fun minuteTicks(wallMillis: () -> Long): Flow<Long> = flow {
    while (true) {
        val now = wallMillis()
        emit(now / MILLIS_PER_MINUTE)
        delay(MILLIS_PER_MINUTE - now % MILLIS_PER_MINUTE)
    }
}

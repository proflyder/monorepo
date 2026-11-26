package dev.proflyder.home.feature.alarms.presentation

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.stateIn

internal const val MILLIS_PER_MINUTE = 60_000L
internal const val SECONDS_PER_MINUTE = 60L

internal fun minuteTicks(wallMillis: () -> Long): Flow<Long> = flow {
    while (true) {
        val now = wallMillis()
        emit(now / MILLIS_PER_MINUTE)
        delay(MILLIS_PER_MINUTE - now % MILLIS_PER_MINUTE)
    }
}

internal fun CoroutineScope.minuteState(wallMillis: () -> Long): StateFlow<Long> =
    minuteTicks(wallMillis).stateIn(this, SharingStarted.WhileSubscribed(), wallMillis() / MILLIS_PER_MINUTE)

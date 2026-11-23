package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.NetworkEnvironment
import kotlin.time.Duration
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.withTimeoutOrNull

internal data class EnvironmentReading(val state: HomeNetworkState, val undetermined: Boolean = false)

@OptIn(ExperimentalCoroutinesApi::class)
internal fun NetworkEnvironment.readings(timeout: Duration): Flow<EnvironmentReading> =
    state.transformLatest { emitWithDeadline(it, timeout) }

internal suspend fun NetworkEnvironment.settledState(timeout: Duration): HomeNetworkState =
    withTimeoutOrNull(timeout) { state.first { it != HomeNetworkState.Unknown } } ?: HomeNetworkState.Unknown

private suspend fun FlowCollector<EnvironmentReading>.emitWithDeadline(state: HomeNetworkState, timeout: Duration) {
    emit(EnvironmentReading(state))
    if (state != HomeNetworkState.Unknown) return
    delay(timeout)
    emit(EnvironmentReading(state, undetermined = true))
}

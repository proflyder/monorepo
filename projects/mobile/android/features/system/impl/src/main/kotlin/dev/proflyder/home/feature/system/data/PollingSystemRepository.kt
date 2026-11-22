package dev.proflyder.home.feature.system.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.feature.system.SystemRepository
import dev.proflyder.home.feature.system.SystemState
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.updateAndGet

internal data class SystemRefreshPolicy(
    val period: Duration = 30.seconds,
    val stopTimeout: Duration = 5.seconds,
)

internal class PollingSystemRepository(
    private val load: suspend () -> Outcome<SystemSnapshot>,
    scope: CoroutineScope,
    policy: SystemRefreshPolicy,
) : SystemRepository {
    private val current = MutableStateFlow(SystemState())
    private val requests = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    @OptIn(ExperimentalCoroutinesApi::class)
    override val state: StateFlow<SystemState> = merge(tickerFlow(policy.period), requests)
        .transformLatest {
            emit(current.updateAndGet { it.copy(loading = true) })
            emit(current.updateAndGet { it.applied(load()) })
        }
        .stateIn(scope, SharingStarted.WhileSubscribed(policy.stopTimeout.inWholeMilliseconds), current.value)

    override fun refresh() {
        requests.tryEmit(Unit)
    }

    private fun SystemState.applied(result: Outcome<SystemSnapshot>): SystemState = when (result) {
        is Outcome.Success -> SystemState(loading = false, snapshot = result.value, error = null)
        is Outcome.Failure -> copy(loading = false, error = result.failure)
    }
}

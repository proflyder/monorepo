package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.data.StalenessPolicy
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignalSink
import dev.proflyder.home.core.data.SyncSignals
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlin.time.Duration.Companion.nanoseconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<SyncSignals>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class AppSyncSignals(
    private val foreground: AppForeground,
    private val scope: CoroutineScope,
    private val policy: StalenessPolicy,
) : SyncSignals, SyncSignalSink, AppStartable {
    private val bus = MutableSharedFlow<SyncSignal>(
        extraBufferCapacity = BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    override val signals: Flow<SyncSignal> = bus.asSharedFlow()

    override fun publish(signal: SyncSignal) {
        bus.tryEmit(signal)
    }

    override fun start() {
        foreground.state
            .runningFold(Transition(), Transition::next)
            .filter { it.returnedAfter(policy) }
            .onEach { publish(SyncSignal.Stale) }
            .launchIn(scope)
    }

    private companion object {
        const val BUFFER = 64
    }
}

internal data class Transition(val previous: ForegroundState? = null, val current: ForegroundState? = null) {
    fun next(state: ForegroundState): Transition = Transition(current, state)

    fun returnedAfter(policy: StalenessPolicy): Boolean {
        val away = previous?.takeUnless { it.foreground } ?: return false
        val back = current?.takeIf { it.foreground } ?: return false
        return policy.isStale((back.sinceElapsedNanos - away.sinceElapsedNanos).nanoseconds)
    }
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<SyncSignalSink>())
@Inject
class AppSyncSignalSink(private val signals: AppSyncSignals) : SyncSignalSink by signals

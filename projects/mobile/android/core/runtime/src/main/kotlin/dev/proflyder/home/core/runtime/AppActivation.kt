package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.data.SyncSignals
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach

interface AppActivation {
    val active: StateFlow<Boolean>

    fun activate()

    suspend fun awaitActive()
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<AppActivation>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class ProcessAppActivation(
    private val foreground: AppForeground,
    private val signals: SyncSignals,
    private val scope: CoroutineScope,
) : AppActivation, AppStartable {
    private val mutableActive = MutableStateFlow(false)

    override val active: StateFlow<Boolean> = mutableActive.asStateFlow()

    override fun start() {
        foreground.state.map { it.foreground }.filter { it }.onEach { activate() }.launchIn(scope)
        signals.signals.onEach { activate() }.launchIn(scope)
    }

    override fun activate() {
        mutableActive.value = true
    }

    override suspend fun awaitActive() {
        active.first { it }
    }
}

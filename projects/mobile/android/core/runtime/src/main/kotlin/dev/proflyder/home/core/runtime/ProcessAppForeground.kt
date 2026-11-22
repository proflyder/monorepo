package dev.proflyder.home.core.runtime

import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import dev.proflyder.home.core.model.Clock
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<AppForeground>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class ProcessAppForeground(
    private val clock: Clock,
) : AppForeground, AppStartable, DefaultLifecycleObserver {
    private val mutableState = MutableStateFlow(ForegroundState(foreground = false, clock.elapsedNanos()))

    override val state: StateFlow<ForegroundState> = mutableState.asStateFlow()

    override fun start() {
        ProcessLifecycleOwner.get().lifecycle.addObserver(this)
    }

    override fun onStart(owner: LifecycleOwner) = moveTo(foreground = true)

    override fun onStop(owner: LifecycleOwner) = moveTo(foreground = false)

    private fun moveTo(foreground: Boolean) {
        mutableState.value = ForegroundState(foreground, clock.elapsedNanos())
    }
}

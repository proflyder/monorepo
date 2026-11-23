package dev.proflyder.home.core.presentation

import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map

class ScreenVisibility {
    private val visibleNow = MutableStateFlow(false)
    private val shows = MutableStateFlow(0)
    private var observers = 0

    val visible: StateFlow<Boolean> = visibleNow.asStateFlow()

    val shown: Flow<Unit> = shows.filter { it > 0 }.map { }

    fun started() {
        observers += 1
        if (observers > 1) return
        visibleNow.value = true
        shows.value += 1
    }

    fun stopped() {
        observers = (observers - 1).coerceAtLeast(0)
        if (observers == 0) visibleNow.value = false
    }
}

class VisibilityReporter(private val visibility: ScreenVisibility) : DefaultLifecycleObserver {
    override fun onStart(owner: LifecycleOwner) = visibility.started()

    override fun onStop(owner: LifecycleOwner) = visibility.stopped()
}

fun LifecycleOwner.reportVisibility(visibility: ScreenVisibility) {
    lifecycle.addObserver(VisibilityReporter(visibility))
}

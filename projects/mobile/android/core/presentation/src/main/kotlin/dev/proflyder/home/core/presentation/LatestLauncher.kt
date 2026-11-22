package dev.proflyder.home.core.presentation

import kotlin.properties.ReadOnlyProperty
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

class LatestLauncher(private val scope: CoroutineScope) {
    private var running: Job? = null

    val isActive: Boolean get() = running?.isActive == true

    fun launch(block: suspend CoroutineScope.() -> Unit): Job {
        running?.cancel()
        return scope.launch(block = block).also { running = it }
    }

    fun cancel() {
        running?.cancel()
    }
}

fun CoroutineScope.latestLauncher(): ReadOnlyProperty<Any?, LatestLauncher> {
    val launcher = LatestLauncher(this)
    return ReadOnlyProperty { _, _ -> launcher }
}

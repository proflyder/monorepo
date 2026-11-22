package dev.proflyder.home.feature.push

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update

internal class InFlight {
    private val running = MutableStateFlow(0)

    val busy: Flow<Boolean> = running.map { it > 0 }.distinctUntilChanged()

    suspend fun <T> run(block: suspend () -> T): T {
        running.update { it + 1 }
        val result = runCatching { block() }
        running.update { it - 1 }
        return result.getOrThrow()
    }
}

package dev.proflyder.home.core.network

import dev.proflyder.home.core.runtime.ForegroundState
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ConnectionHealthTest {
    private val foreground = MutableStateFlow(ForegroundState(foreground = false, sinceElapsedNanos = 0))
    private val network = MutableStateFlow<Long?>(3L)
    private var signals = 0

    @Test
    fun returningAfterALongBackgroundIsAStaleSignal() = runTest(UnconfinedTestDispatcher()) {
        observe()
        move(foreground = true, atSeconds = 1)
        move(foreground = false, atSeconds = 10)
        move(foreground = true, atSeconds = 20)
        assertEquals(0, signals)
        move(foreground = false, atSeconds = 30)
        move(foreground = true, atSeconds = 50)
        assertEquals(1, signals)
    }

    @Test
    fun switchingTheDefaultNetworkIsAStaleSignal() = runTest(UnconfinedTestDispatcher()) {
        observe()
        network.value = 7L
        network.value = null
        assertEquals(2, signals)
    }

    private fun TestScope.observe() {
        staleConnectionSignals(foreground, network).onEach { signals += 1 }.launchIn(backgroundScope)
    }

    private fun move(foreground: Boolean, atSeconds: Long) {
        this.foreground.value = ForegroundState(foreground, atSeconds.seconds.inWholeNanoseconds)
    }
}

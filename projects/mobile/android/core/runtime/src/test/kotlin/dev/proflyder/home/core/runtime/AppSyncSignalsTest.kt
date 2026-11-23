package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.data.StalenessPolicy
import dev.proflyder.home.core.data.SyncSignal
import kotlin.time.Duration.Companion.minutes
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
class AppSyncSignalsTest {
    private val state = MutableStateFlow(ForegroundState(foreground = true, sinceElapsedNanos = 0))
    private val foreground = object : AppForeground {
        override val state = this@AppSyncSignalsTest.state
    }

    @Test
    fun onlyAReturnAfterTheThresholdIsStale() = runTest {
        val scope = TestScope(UnconfinedTestDispatcher(testScheduler))
        val signals = AppSyncSignals(foreground, scope, StalenessPolicy(5.minutes))
        val seen = mutableListOf<SyncSignal>()
        signals.signals.onEach { seen += it }.launchIn(scope)
        signals.start()
        move(foreground = false, at = 1.minutes.inWholeNanoseconds)
        move(foreground = true, at = 2.minutes.inWholeNanoseconds)
        assertEquals(emptyList<SyncSignal>(), seen)
        move(foreground = false, at = 3.minutes.inWholeNanoseconds)
        move(foreground = true, at = 9.minutes.inWholeNanoseconds)
        assertEquals(listOf<SyncSignal>(SyncSignal.Stale), seen)
    }

    private fun move(foreground: Boolean, at: Long) {
        state.value = ForegroundState(foreground, at)
    }
}

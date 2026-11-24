package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.testing.FakeSyncSignals
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProcessAppActivationTest {
    private class FakeForeground(foreground: Boolean = false) : AppForeground {
        override val state = MutableStateFlow(ForegroundState(foreground, 0))
    }

    @Test
    fun theProcessActivatesWhenTheAppComesToTheForeground() = runTest {
        val foreground = FakeForeground()
        val activation = ProcessAppActivation(foreground, FakeSyncSignals(), backgroundScope)
        activation.start()
        runCurrent()
        assertFalse(activation.active.value)

        foreground.state.value = ForegroundState(foreground = true, sinceElapsedNanos = 0)
        runCurrent()
        assertTrue(activation.active.value)
    }

    @Test
    fun aSyncSignalActivatesTheProcessWithoutAnyActivity() = runTest {
        val signals = FakeSyncSignals()
        val activation = ProcessAppActivation(FakeForeground(), signals, backgroundScope)
        activation.start()
        runCurrent()
        assertFalse(activation.active.value)

        signals.publish(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.UPSERT, "alarm-1"))
        runCurrent()
        assertTrue(activation.active.value)
    }

    @Test
    fun awaitActiveSuspendsUntilTheFirstRealTrigger() = runTest {
        val activation = ProcessAppActivation(FakeForeground(), FakeSyncSignals(), backgroundScope)
        activation.start()
        runCurrent()
        var resumed = false
        backgroundScope.launch {
            activation.awaitActive()
            resumed = true
        }
        runCurrent()
        assertFalse(resumed)

        activation.activate()
        runCurrent()
        assertTrue(resumed)
    }
}

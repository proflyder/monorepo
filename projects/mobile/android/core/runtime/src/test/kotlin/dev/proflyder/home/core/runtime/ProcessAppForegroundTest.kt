package dev.proflyder.home.core.runtime

import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import dev.proflyder.home.core.testing.FakeClock
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ProcessAppForegroundTest {
    @Test
    fun foregroundTransitionsAreStampedWithTheClock() {
        val clock = FakeClock(elapsedNanos = 10L)
        val foreground = ProcessAppForeground(clock)
        assertEquals(ForegroundState(foreground = false, sinceElapsedNanos = 10L), foreground.state.value)
        clock.elapsedNanos = 50L
        foreground.onStart(owner)
        assertEquals(ForegroundState(foreground = true, sinceElapsedNanos = 50L), foreground.state.value)
        assertEquals(25L, foreground.state.value.foregroundFor(75L))
        foreground.onStop(owner)
        assertNull(foreground.state.value.foregroundFor(90L))
    }

    private val owner = object : LifecycleOwner {
        override val lifecycle: Lifecycle get() = error("not observed in this test")
    }
}

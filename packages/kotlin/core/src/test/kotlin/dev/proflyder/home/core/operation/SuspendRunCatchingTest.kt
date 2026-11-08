package dev.proflyder.home.core.operation

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SuspendRunCatchingTest {
    @Test
    fun `returns success for a suspending block`() = runBlocking {
        val result = suspendRunCatching { 42 }

        assertEquals(42, result.getOrNull())
    }

    @Test
    fun `returns failure for a thrown exception`() = runBlocking {
        val result = suspendRunCatching { error("boom") }

        assertTrue(result.exceptionOrNull() is IllegalStateException)
    }

    @Test
    fun `rethrows cancellation`() {
        val result = runCatching {
            runBlocking { suspendRunCatching<Unit> { throw CancellationException("stop") } }
        }

        assertTrue(result.exceptionOrNull() is CancellationException)
    }
}

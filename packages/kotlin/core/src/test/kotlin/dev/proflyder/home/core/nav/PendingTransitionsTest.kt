package dev.proflyder.home.core.nav

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PendingTransitionsTest {
    @Test
    fun queuesWhileGatedAndDrainsInOrderOnlyWhenResumed() {
        val queue = PendingTransitions<String>()
        val delivered = mutableListOf<String>()

        queue.offer("alarms")
        queue.offer("editor")
        // Not resumed yet: nothing reaches the FragmentManager.
        assertTrue(delivered.isEmpty())
        assertEquals(2, queue.size)

        // Resume drains both, in order, exactly once.
        queue.drain(delivered::add)
        assertEquals(listOf("alarms", "editor"), delivered)
        assertEquals(0, queue.size)

        queue.drain(delivered::add)
        assertEquals(listOf("alarms", "editor"), delivered)
    }

    @Test
    fun collapsesConsecutiveDuplicates() {
        val queue = PendingTransitions<String>()
        assertTrue(queue.offer("tab:timers"))
        assertTrue(queue.offer("tab:timers"))
        assertEquals(1, queue.size)
        assertTrue(queue.offer("tab:alarms"))
        assertEquals(2, queue.size)
    }

    @Test
    fun boundedQueueRejectsNewTransitionsInsteadOfSilentlyDroppingOld() {
        val queue = PendingTransitions<Int>(capacity = 2)
        assertTrue(queue.offer(1))
        assertTrue(queue.offer(2))
        assertFalse(queue.offer(3))
        assertEquals(listOf(1, 2), queue.snapshot())

        val delivered = mutableListOf<Int>()
        queue.drain(delivered::add)
        assertEquals(listOf(1, 2), delivered)
        assertTrue(queue.isEmpty)
    }

    @Test
    fun failingConsumerKeepsRemainingCommandsForRetry() {
        val queue = PendingTransitions<String>()
        queue.offer("a")
        queue.offer("b")
        val delivered = mutableListOf<String>()

        var attempts = 0
        try {
            queue.drain { command ->
                attempts++
                if (command == "b") error("state saved again")
                delivered += command
            }
        } catch (_: IllegalStateException) {
            // expected
        }

        assertEquals(listOf("a"), delivered)
        assertEquals(1, queue.size)
        assertEquals(listOf("b"), queue.snapshot())
        assertEquals(2, attempts)

        queue.drain(delivered::add)
        assertEquals(listOf("a", "b"), delivered)
        assertTrue(queue.isEmpty)
    }
}

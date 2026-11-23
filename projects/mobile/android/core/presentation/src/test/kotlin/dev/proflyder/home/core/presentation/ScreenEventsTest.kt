package dev.proflyder.home.core.presentation

import androidx.lifecycle.Lifecycle
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class ScreenEventsTest {
    private data class Plain(val text: String)

    private data class Started(val text: String) : DeliveredWhenStarted

    @Test
    fun anEventStaysQueuedUntilItIsConsumed() = runTest {
        val events = ScreenEvents<Plain>()
        events.emit(Plain("a"))
        events.emit(Plain("a"))

        val head = events.next.first()
        assertEquals(head, events.next.first())
        events.consume(head)

        assertEquals(Plain("a"), events.next.first().event)
        assertEquals(Plain("a"), events.consumed.first())
    }

    @Test
    fun navigationAndDialogsWaitForResumedByDefault() {
        assertEquals(Lifecycle.State.RESUMED, Plain("x").deliveryState)
        assertEquals(Lifecycle.State.STARTED, Started("x").deliveryState)
    }
}

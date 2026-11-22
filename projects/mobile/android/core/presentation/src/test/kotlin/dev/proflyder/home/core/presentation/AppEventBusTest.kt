package dev.proflyder.home.core.presentation

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AppEventBusTest {
    private data class ScheduleChanged(val id: String) : AppEvent

    private data object ThemeChanged : AppEvent

    @Test
    fun subscribersReceiveOnlyTheirTypedEvents() = runTest(UnconfinedTestDispatcher()) {
        val bus: AppEventBus = SharedFlowAppEventBus()
        val received = mutableListOf<ScheduleChanged>()
        backgroundScope.launch { bus.eventsOf<ScheduleChanged>().collect { received += it } }
        bus.publish(ScheduleChanged("alarm-1"))
        bus.publish(ThemeChanged)
        bus.publish(ScheduleChanged("timer-2"))
        assertEquals(listOf(ScheduleChanged("alarm-1"), ScheduleChanged("timer-2")), received)
    }
}

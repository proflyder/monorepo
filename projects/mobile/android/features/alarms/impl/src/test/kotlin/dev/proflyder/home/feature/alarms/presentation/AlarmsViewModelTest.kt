package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.SharedFlowAppEventBus
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AlarmsViewModelTest {
    private val repository = FakeAlarmRepository(
        listOf(schedule("late", hour = 9), schedule("early", hour = 6), schedule("once", hour = 8, repeat = RepeatKind.ONCE)),
    )
    private val bus = SharedFlowAppEventBus()

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun viewModel() = AlarmsViewModel(repository, SnoozeAlarm(repository), bus)

    @Test
    fun loadsWhenShownAndSortsByTheDomainOrder() = runTest {
        val viewModel = viewModel()
        assertEquals(0, repository.listCalls)
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { viewModel.state.collect {} }
        assertEquals(listOf("early", "once", "late"), viewModel.state.value.alarms.map { it.id })
        assertFalse(viewModel.state.value.load.refreshing)
    }

    @Test
    fun anEditorChangeRefreshesTheListInTheBackground() {
        val viewModel = viewModel()
        bus.publish(AlarmsChanged)
        assertEquals(1, repository.listCalls)
        assertEquals(3, viewModel.state.value.alarms.size)
    }

    @Test
    fun disablingARepeatingAlarmAsksAndKeepsItEnabled() = runTest {
        val viewModel = viewModel()
        viewModel.dispatch(AlarmsIntent.Refresh(RefreshMode.HARD))
        viewModel.dispatch(AlarmsIntent.Toggle("late", enabled = false))
        assertEquals(AlarmsEvent.AskDisable("late"), viewModel.events.first())
        assertTrue(viewModel.state.value.alarms.first { it.id == "late" }.enabled)
        viewModel.dispatch(AlarmsIntent.Toggle("once", enabled = false))
        assertFalse(viewModel.state.value.alarms.first { it.id == "once" }.enabled)
    }

    @Test
    fun selectionClickTogglesAndBulkDeleteClearsIt() = runTest {
        val viewModel = viewModel()
        viewModel.dispatch(AlarmsIntent.Refresh(RefreshMode.HARD))
        viewModel.dispatch(AlarmsIntent.LongClick("late"))
        viewModel.dispatch(AlarmsIntent.Click("early"))
        assertEquals(setOf("late", "early"), viewModel.state.value.selectedIds)
        viewModel.dispatch(AlarmsIntent.DeleteSelected)
        assertEquals(listOf("once"), viewModel.state.value.alarms.map { it.id })
        assertFalse(viewModel.state.value.selectionMode)
    }

    @Test
    fun aFailedRefreshWithItemsIsAnEventNotAState() = runTest {
        val viewModel = viewModel()
        viewModel.dispatch(AlarmsIntent.Refresh(RefreshMode.HARD))
        repository.failure = RemoteFailure.Timeout
        viewModel.dispatch(AlarmsIntent.Refresh(RefreshMode.SOFT))
        assertEquals(AlarmsEvent.Failure(RemoteFailure.Timeout), viewModel.events.first())
        assertEquals(3, viewModel.state.value.alarms.size)
    }
}

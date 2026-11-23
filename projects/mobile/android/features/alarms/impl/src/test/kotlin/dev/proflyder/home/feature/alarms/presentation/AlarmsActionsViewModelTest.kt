package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AlarmsActionsViewModelTest {
    private val remote = FakeAlarmRemote(
        listOf(schedule("daily", hour = 9), schedule("once", hour = 8, repeat = RepeatKind.ONCE)),
    )

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val repository by lazy { remote.repository() }

    private fun actions() = AlarmsActionsViewModel(repository, SnoozeAlarm(repository))

    @Test
    fun disablingARepeatingAlarmAsksAndKeepsItEnabled() = runTest {
        val actions = actions()
        actions.dispatch(AlarmActionIntent.Toggle("daily", enabled = false))
        assertEquals(AlarmActionEvent.AskDisable("daily"), actions.events.first())
        assertTrue(checkNotNull(repository["daily"]).enabled)
        actions.dispatch(AlarmActionIntent.Toggle("once", enabled = false))
        assertFalse(checkNotNull(repository["once"]).enabled)
    }

    @Test
    fun aFailedToggleRollsTheStoreBack() = runTest {
        val actions = actions()
        remote.failure = RemoteFailure.Timeout
        actions.dispatch(AlarmActionIntent.Toggle("once", enabled = false))
        assertTrue(checkNotNull(repository["once"]).enabled)
        assertEquals(AlarmActionEvent.Failure(RemoteFailure.Timeout), actions.events.first())
    }

    @Test
    fun deletionAsksFirstThenOffersUndoThatRestoresTheAlarm() = runTest {
        val actions = actions()
        actions.dispatch(AlarmActionIntent.RequestDelete("daily"))
        assertEquals(AlarmActionEvent.ConfirmDelete("daily"), actions.events.first())
        actions.dispatch(AlarmActionIntent.Delete("daily"))
        assertNull(repository["daily"])
        assertEquals("daily", actions.state.value.undo?.id)
        actions.dispatch(AlarmActionIntent.Undo)
        assertNull(actions.state.value.undo)
        assertEquals(listOf("once", "daily"), repository.items.value.map { it.label })
    }

    @Test
    fun theListAndTheHomeSectionShareOneStore() {
        val list = AlarmsViewModel(repository)
        val section = UpcomingAlarmsViewModel(repository, SnoozeAlarm(repository), ServerClock(FakeClock()))
        val actions = actions()
        remote.alarms = remote.alarms.map { it.copy(nextDueUnix = 100) }
        repository.refresh(dev.proflyder.home.core.data.RefreshMode.SOFT)
        assertEquals(2, section.state.value.alarms.size)
        actions.dispatch(AlarmActionIntent.Toggle("once", enabled = false))
        assertFalse(list.state.value.alarms.first { it.id == "once" }.enabled)
        assertEquals(listOf("daily"), section.state.value.alarms.map { it.alarm.id })
    }
}

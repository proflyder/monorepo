package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.testing.FakeSyncSignals
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.toAlarmDraft
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
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AlarmsViewModelTest {
    private val remote = FakeAlarmRemote(
        listOf(schedule("late", hour = 9), schedule("early", hour = 6), schedule("once", hour = 8, repeat = RepeatKind.ONCE)),
    )

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val signals = FakeSyncSignals()

    private val repository by lazy { remote.repository(signals) }

    private fun viewModel() = AlarmsViewModel(repository)

    @Test
    fun showsTheStoreSortedByTheDomainOrder() {
        val viewModel = viewModel()
        assertEquals(1, remote.listCalls)
        assertEquals(listOf("early", "once", "late"), viewModel.state.value.alarms.map { it.id })
        assertFalse(viewModel.state.value.load.refreshing)
    }

    @Test
    fun aChangeMadeElsewhereReachesTheListThroughTheStoreWithoutAReload() = runTest {
        val viewModel = viewModel()
        repository.create(schedule("x", hour = 7).toAlarmDraft(), "key")
        assertEquals(1, remote.listCalls)
        assertEquals(4, viewModel.state.value.alarms.size)
    }

    @Test
    fun showingTheScreenAgainMakesNoRequest() {
        val viewModel = viewModel()
        viewModel.visibility.started()
        viewModel.visibility.stopped()
        viewModel.visibility.started()
        assertEquals(1, remote.listCalls)
    }

    @Test
    fun aSyncUpsertFetchesOneAlarmIntoTheStoreWithoutReloadingTheList() {
        val viewModel = viewModel()
        remote.alarms = remote.alarms + schedule("pushed", hour = 7)
        signals.publish(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.UPSERT, "pushed"))
        assertEquals(listOf("pushed"), remote.gets)
        assertEquals(1, remote.listCalls)
        assertTrue(viewModel.state.value.alarms.any { it.id == "pushed" })
    }

    @Test
    fun aSyncDeleteRemovesTheAlarmWithoutAnyRequest() {
        val viewModel = viewModel()
        signals.publish(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.DELETE, "late"))
        assertEquals(1, remote.listCalls)
        assertTrue(remote.gets.isEmpty())
        assertEquals(listOf("early", "once"), viewModel.state.value.alarms.map { it.id })
    }

    @Test
    fun rowsAreKeptWhenOnlyUnrelatedFieldsChange() {
        val viewModel = viewModel()
        val rows = viewModel.state.value.rows
        viewModel.dispatch(AlarmsIntent.Refresh(RefreshMode.HARD))
        assertSame(rows, viewModel.state.value.rows)
        viewModel.dispatch(AlarmsIntent.LongClick("late"))
        val selected = viewModel.state.value.rows
        assertNotSame(rows, selected)
        assertTrue(selected.first { it.alarm.id == "late" }.selected)
        assertTrue(viewModel.state.value.selection.active)
    }

    @Test
    fun selectionClickTogglesAndBulkDeleteClearsIt() = runTest {
        val viewModel = viewModel()
        viewModel.dispatch(AlarmsIntent.LongClick("late"))
        viewModel.dispatch(AlarmsIntent.Click("early"))
        assertEquals(setOf("late", "early"), viewModel.state.value.selectedIds)
        viewModel.dispatch(AlarmsIntent.DeleteSelected)
        assertEquals(listOf("once"), viewModel.state.value.alarms.map { it.id })
        assertFalse(viewModel.state.value.selection.active)
    }

    @Test
    fun aFailedRefreshWithItemsIsAnEventNotAState() = runTest {
        val viewModel = viewModel()
        remote.failure = RemoteFailure.Timeout
        viewModel.dispatch(AlarmsIntent.Refresh(RefreshMode.SOFT))
        assertEquals(AlarmsEvent.Failure(RemoteFailure.Timeout), viewModel.events.first())
        assertEquals(3, viewModel.state.value.alarms.size)
    }
}

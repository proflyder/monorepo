package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.EditorDraftSession
import dev.proflyder.home.core.presentation.SharedFlowAppEventBus
import dev.proflyder.home.core.presentation.eventsOf
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.domain.SaveAlarm
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
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AlarmEditorViewModelTest {
    private val repository = FakeAlarmRepository(listOf(schedule("a1", hour = 6, minute = 45, label = "Gym")))
    private val bus = SharedFlowAppEventBus()

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun viewModel(state: SavedStateHandle, alarmId: String? = null) = AlarmEditorViewModel(
        repository = repository,
        saveAlarm = SaveAlarm(repository),
        bus = bus,
        session = EditorDraftSession(state, "alarm", AlarmDraftCodec, AlarmEditorCompletion.entries) { "https://home" },
        defaultLabel = "Alarm",
        alarmId = alarmId,
    )

    @Test
    fun loadsTheEditedAlarmIntoTheForm() {
        val state = viewModel(SavedStateHandle(), alarmId = "a1").state.value
        assertEquals("Gym", state.label)
        assertEquals(6, state.hour)
        assertTrue(state.editing)
        assertTrue(state.status.idle)
    }

    @Test
    fun aTimedOutCreateIsRetriedWithTheSameKeyAfterRestore() {
        val saved = SavedStateHandle()
        repository.failure = RemoteFailure.Timeout
        val first = viewModel(saved)
        first.dispatch(AlarmEditorIntent.SetLabel("Run"))
        first.dispatch(AlarmEditorIntent.Save)
        assertEquals(RemoteFailure.Timeout, first.state.value.status.error)
        repository.failure = null
        val restored = viewModel(SavedStateHandle(saved.keys().associateWith { saved.get<Any?>(it) }))
        assertEquals("Run", restored.state.value.label)
        restored.dispatch(AlarmEditorIntent.Save)
        assertEquals(repository.createKeys[0], repository.createKeys[1])
    }

    @Test
    fun savingCompletesOnceAndRefreshesTheList() = runTest {
        val changed = mutableListOf<AlarmsChanged>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { bus.eventsOf<AlarmsChanged>().collect { changed += it } }
        val viewModel = viewModel(SavedStateHandle(), alarmId = "a1")
        viewModel.dispatch(AlarmEditorIntent.Save)
        assertEquals(AlarmEditorEvent.Completed(AlarmEditorCompletion.SAVED), viewModel.events.first())
        assertEquals(listOf(AlarmsChanged), changed)
    }
}

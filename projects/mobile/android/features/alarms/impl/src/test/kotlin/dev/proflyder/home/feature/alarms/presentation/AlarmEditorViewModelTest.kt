package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.RouteNotice
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.EditorDraftSession
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.domain.SaveAlarm
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
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
    private val remote = FakeAlarmRemote(listOf(schedule("a1", hour = 6, minute = 45, label = "Gym")))
    private val repository by lazy { remote.repository() }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun viewModel(state: SavedStateHandle, alarmId: String? = null) = AlarmEditorViewModel(
        repository = repository,
        saveAlarm = SaveAlarm(repository),
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
        remote.failure = RemoteFailure.Timeout
        val first = viewModel(saved)
        first.dispatch(AlarmEditorIntent.SetLabel("Run"))
        first.dispatch(AlarmEditorIntent.Save)
        assertEquals(RemoteFailure.Timeout, first.state.value.status.error)
        remote.failure = null
        val restored = viewModel(SavedStateHandle(saved.keys().associateWith { saved.get<Any?>(it) }))
        assertEquals("Run", restored.state.value.label)
        restored.dispatch(AlarmEditorIntent.Save)
        assertEquals(remote.createKeys[0], remote.createKeys[1])
    }

    @Test
    fun savingCompletesOnceAndUpdatesTheSharedStore() = runTest {
        val viewModel = viewModel(SavedStateHandle(), alarmId = "a1")
        viewModel.dispatch(AlarmEditorIntent.SetLabel("Run"))
        viewModel.dispatch(AlarmEditorIntent.Save)
        assertEquals(AlarmEditorEvent.Completed(RouteNotice(R.string.alarms_saved)), viewModel.events.first())
        assertEquals("Run", repository["a1"]?.label)
        assertEquals(1, remote.listCalls)
    }

    @Test
    fun theEditorClosesWhenItsAlarmIsDeletedElsewhere() = runTest {
        val viewModel = viewModel(SavedStateHandle(), alarmId = "a1")
        repository.delete("a1")
        assertEquals(AlarmEditorEvent.OpenRoute(Route.Back), viewModel.events.first())
    }

    @Test
    fun creatingAnAlarmOffersToOpenIt() = runTest {
        val viewModel = viewModel(SavedStateHandle())
        viewModel.dispatch(AlarmEditorIntent.Save)
        val notice = (viewModel.events.first() as AlarmEditorEvent.Completed).notice
        val created = checkNotNull(viewModel.state.value.existing).id
        assertEquals(Route.Screen(AlarmEditorDestination(created)), notice.route)
    }
}

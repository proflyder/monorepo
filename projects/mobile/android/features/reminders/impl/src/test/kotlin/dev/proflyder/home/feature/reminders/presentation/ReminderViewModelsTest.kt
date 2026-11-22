package dev.proflyder.home.feature.reminders.presentation

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.presentation.EditorDraftSession
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.SharedFlowAppEventBus
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.reminders.ReminderDraft
import dev.proflyder.home.feature.reminders.ReminderEditorCompletion
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.reminders.domain.SaveReminder
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
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ReminderViewModelsTest {
    private val repository = FakeReminderRepository(
        listOf(
            reminder("late", "2026-11-23", hour = 10),
            reminder("early", "2026-11-23", hour = 9),
            reminder("first", "2026-11-22", hour = 18),
        ),
    )
    private val bus = SharedFlowAppEventBus()

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun remindersAreOrderedByDateThenTimeAndRefreshOnEditorChanges() {
        val viewModel = RemindersViewModel(repository, bus)
        bus.publish(RemindersChanged)
        assertEquals(listOf("first", "early", "late"), viewModel.state.value.reminders.map { it.id })
    }

    @Test
    fun aToggleIsAppliedOnlyFromTheServerAnswer() = runTest {
        val viewModel = RemindersViewModel(repository, bus)
        viewModel.dispatch(RemindersIntent.Refresh(RefreshMode.HARD))
        repository.failure = RemoteFailure.Timeout
        viewModel.dispatch(RemindersIntent.Toggle("early", enabled = false))
        assertEquals(RemindersEvent.Failure(RemoteFailure.Timeout), viewModel.events.first())
        assertEquals(true, viewModel.state.value.reminders.first { it.id == "early" }.enabled)
    }

    @Test
    fun theEditorRestoresItsDraftAndCompletesAfterDelete() = runTest {
        val saved = SavedStateHandle()
        val first = editor(saved, "early")
        first.dispatch(ReminderEditorIntent.SetLabel("Call"))
        val restored = editor(SavedStateHandle(saved.keys().associateWith { saved.get<Any?>(it) }), "early")
        assertEquals("Call", restored.state.value.label)
        restored.dispatch(ReminderEditorIntent.Delete)
        assertEquals(ReminderEditorEvent.Completed(ReminderEditorCompletion.DELETED), restored.events.first())
        assertFalse(restored.state.value.status.busy)
    }

    private fun editor(state: SavedStateHandle, id: String?) = ReminderEditorViewModel(
        repository = repository,
        saveReminder = SaveReminder(repository),
        bus = bus,
        session = EditorDraftSession(state, "reminder", ReminderDraftCodec, ReminderEditorCompletion.entries) { "https://home" },
        reminderId = id,
    )

    private fun reminder(id: String, date: String, hour: Int) =
        schedule(id, hour = hour, kind = ScheduleKind.REMINDER, onceDate = date)
}

internal class FakeReminderRepository(private var reminders: List<Schedule>) : ReminderRepository {
    var failure: RemoteFailure? = null

    override suspend fun list(): Outcome<List<Schedule>> = answer(reminders)

    override suspend fun get(id: String): Outcome<Schedule> = answer(reminders.first { it.id == id })

    override suspend fun create(draft: ReminderDraft, idempotencyKey: String): Outcome<Schedule> = answer(reminders.first())

    override suspend fun update(existing: Schedule, draft: ReminderDraft): Outcome<Schedule> = answer(existing)

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> =
        answer(reminders.first { it.id == id }.copy(enabled = enabled))

    override suspend fun delete(id: String): Outcome<Boolean> = answer(true)

    private fun <T> answer(value: T): Outcome<T> = failure?.let { Outcome.Failure(it) } ?: Outcome.Success(value)
}

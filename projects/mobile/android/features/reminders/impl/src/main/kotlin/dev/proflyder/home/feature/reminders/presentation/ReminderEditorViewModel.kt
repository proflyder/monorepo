package dev.proflyder.home.feature.reminders.presentation

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.presentation.AppEventBus
import dev.proflyder.home.core.presentation.EditorDraftCodec
import dev.proflyder.home.core.presentation.EditorDraftSession
import dev.proflyder.home.core.presentation.EditorStatus
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.feature.reminders.ReminderDraft
import dev.proflyder.home.feature.reminders.ReminderEditorCompletion
import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.feature.reminders.ReminderEditorDraft
import dev.proflyder.home.feature.reminders.ReminderEditorDraftCodec
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.reminders.domain.SaveReminder
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.time.LocalDate

internal data class ReminderEditorUiState(
    val existing: Schedule? = null,
    val label: String = "",
    val date: LocalDate = LocalDate.now(),
    val hour: Int = DEFAULT_HOUR,
    val minute: Int = 0,
    val enabled: Boolean = true,
    val status: EditorStatus = EditorStatus(),
    val editing: Boolean = false,
) {
    private companion object {
        const val DEFAULT_HOUR = 9
    }
}

internal sealed interface ReminderEditorIntent {
    data class SetLabel(val value: String) : ReminderEditorIntent
    data class SetDate(val value: LocalDate) : ReminderEditorIntent
    data class SetTime(val hour: Int, val minute: Int) : ReminderEditorIntent
    data class SetEnabled(val value: Boolean) : ReminderEditorIntent
    data object PickDate : ReminderEditorIntent
    data object PickTime : ReminderEditorIntent
    data object Save : ReminderEditorIntent
    data object RequestDelete : ReminderEditorIntent
    data object Delete : ReminderEditorIntent
    data object CompletionShown : ReminderEditorIntent
}

internal sealed interface ReminderEditorEvent {
    data class PickDate(val selected: LocalDate) : ReminderEditorEvent
    data class PickTime(val hour: Int, val minute: Int) : ReminderEditorEvent
    data object ConfirmDelete : ReminderEditorEvent
    data class Completed(val completion: ReminderEditorCompletion) : ReminderEditorEvent
}

internal object ReminderDraftCodec : EditorDraftCodec<ReminderEditorDraft> {
    override fun encode(draft: ReminderEditorDraft): Map<String, Any?> = ReminderEditorDraftCodec.toMap(draft)

    override fun decode(values: Map<String, Any?>): ReminderEditorDraft? = ReminderEditorDraftCodec.fromMap(values)
}

internal typealias ReminderDraftSession = EditorDraftSession<ReminderEditorDraft, ReminderDraft, ReminderEditorCompletion>

internal class ReminderEditorViewModel(
    private val repository: ReminderRepository,
    private val saveReminder: SaveReminder,
    private val bus: AppEventBus,
    private val session: ReminderDraftSession,
    reminderId: String?,
) : StateViewModel<ReminderEditorUiState, ReminderEditorIntent, ReminderEditorEvent>(
    session.restored?.toUiState() ?: ReminderEditorUiState(editing = reminderId != null),
) {
    init {
        session.restored?.takeIf { it.existingId == null }?.let { session.adoptRestoredCreate(it.toUiState().toDraft()) }
        session.pendingCompletion?.let { emit(ReminderEditorEvent.Completed(it)) }
        start(reminderId)
    }

    override fun handle(intent: ReminderEditorIntent) {
        when (intent) {
            is ReminderEditorIntent.SetLabel -> edit { copy(label = intent.value) }
            is ReminderEditorIntent.SetDate -> edit { copy(date = intent.value) }
            is ReminderEditorIntent.SetTime -> edit { copy(hour = intent.hour, minute = intent.minute) }
            is ReminderEditorIntent.SetEnabled -> edit { copy(enabled = intent.value) }
            ReminderEditorIntent.PickDate -> emit(ReminderEditorEvent.PickDate(currentState.date))
            ReminderEditorIntent.PickTime -> emit(ReminderEditorEvent.PickTime(currentState.hour, currentState.minute))
            ReminderEditorIntent.Save -> save()
            ReminderEditorIntent.RequestDelete -> emit(ReminderEditorEvent.ConfirmDelete)
            ReminderEditorIntent.Delete -> delete()
            ReminderEditorIntent.CompletionShown -> session.consumeCompletion()
        }
    }

    private fun start(reminderId: String?) {
        if (session.restored != null) return
        if (reminderId == null) return session.persist(currentState.toEditorDraft())
        updateState { copy(status = status.loadingStarted()) }
        launch {
            when (val result = repository.get(reminderId)) {
                is Outcome.Success -> loaded(result.value)
                is Outcome.Failure -> updateState { copy(status = status.loadFailed(result.failure)) }
            }
        }
    }

    private fun loaded(reminder: Schedule) {
        updateState { reminder.toUiState() }
        session.persist(currentState.toEditorDraft())
    }

    private fun edit(transform: ReminderEditorUiState.() -> ReminderEditorUiState) {
        updateState(transform)
        session.edited(currentState.toEditorDraft())
    }

    private fun save() {
        val current = currentState
        if (current.status.saving) return
        val draft = current.toDraft()
        updateState { copy(status = status.savingStarted()) }
        launch {
            val result = session.submit(draft.takeIf { current.existing == null }) { key ->
                saveReminder(current.existing, draft, key)
            }
            if (result is Outcome.Success) updateState { copy(existing = result.value) }
            finish(result, ReminderEditorCompletion.SAVED)
        }
    }

    private fun delete() {
        val existing = currentState.existing ?: return
        if (currentState.status.saving) return
        updateState { copy(status = status.savingStarted()) }
        launch { finish(repository.delete(existing.id), ReminderEditorCompletion.DELETED) }
    }

    private fun finish(result: Outcome<*>, completion: ReminderEditorCompletion) {
        val failure = (result as? Outcome.Failure)?.failure
        updateState { copy(status = status.settled(failure)) }
        if (failure != null) return
        session.persist(currentState.toEditorDraft())
        session.complete(completion)
        bus.publish(RemindersChanged)
        emit(ReminderEditorEvent.Completed(completion))
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<SavedStateViewModelFactory>())
@ClassKey(ReminderEditorViewModel::class)
@Inject
class ReminderEditorViewModelSource internal constructor(
    private val repository: ReminderRepository,
    private val saveReminder: SaveReminder,
    private val settings: ConnectionSettingsStore,
    private val bus: AppEventBus,
) : SavedStateViewModelFactory {
    override fun create(savedState: SavedStateHandle): ViewModel = build(savedState)

    private fun build(savedState: SavedStateHandle) = ReminderEditorViewModel(
        repository = repository,
        saveReminder = saveReminder,
        bus = bus,
        session = EditorDraftSession(savedState, KEY_PREFIX, ReminderDraftCodec, ReminderEditorCompletion.entries) {
            settings.config.value.endpoint?.baseUrl().orEmpty()
        },
        reminderId = savedState.get<String>(ReminderEditorDestination.ARG_REMINDER_ID),
    )

    private companion object {
        const val KEY_PREFIX = "reminder"
    }
}

internal fun ReminderEditorUiState.toDraft(): ReminderDraft =
    ReminderDraft(label = label, date = date, hour = hour, minute = minute, enabled = enabled)

internal fun ReminderEditorUiState.toEditorDraft(): ReminderEditorDraft = ReminderEditorDraft(
    existingId = existing?.id,
    existingRevision = existing?.revision,
    label = label,
    date = date,
    hour = hour,
    minute = minute,
    enabled = enabled,
)

internal fun Schedule.toUiState(): ReminderEditorUiState = ReminderEditorUiState(
    existing = this,
    label = label,
    date = runCatching { LocalDate.parse(onceDate) }.getOrDefault(LocalDate.now()),
    hour = at.hour,
    minute = at.minute,
    enabled = enabled,
    editing = true,
)

internal fun ReminderEditorDraft.toUiState(): ReminderEditorUiState = ReminderEditorUiState(
    existing = restoredExisting(),
    label = label,
    date = date,
    hour = hour,
    minute = minute,
    enabled = enabled,
    editing = existingId != null,
)

internal fun ReminderEditorDraft.restoredExisting(): Schedule? {
    val id = existingId ?: return null
    return Schedule(
        id = id,
        label = label,
        kind = ScheduleKind.REMINDER,
        at = WallTime(hour, minute),
        onceDate = date.toString(),
        repeat = RepeatKind.ONCE,
        weekdays = emptyList(),
        nextDueUnix = 0,
        enabled = enabled,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = existingRevision ?: 0,
        createdUnix = 0,
        updatedUnix = 0,
    )
}

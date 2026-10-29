package dev.proflyder.home.feature.reminders.presentation

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.model.isAmbiguousRetryable
import dev.proflyder.home.core.operation.EditorCompletion
import dev.proflyder.home.core.operation.IdempotentRequestKey
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.reminders.ReminderDraft
import dev.proflyder.home.feature.reminders.ReminderEditorCompletion
import dev.proflyder.home.feature.reminders.ReminderEditorDraft
import dev.proflyder.home.feature.reminders.ReminderEditorDraftCodec
import dev.proflyder.home.feature.reminders.domain.DeleteReminder
import dev.proflyder.home.feature.reminders.domain.LoadReminder
import dev.proflyder.home.feature.reminders.domain.SaveReminder
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

data class ReminderEditorUiState(
    val loading: Boolean = false,
    val saving: Boolean = false,
    val existing: Schedule? = null,
    val label: String = "",
    val date: LocalDate = LocalDate.now(),
    val hour: Int = 9,
    val minute: Int = 0,
    val timezone: String = ZoneId.systemDefault().id,
    val enabled: Boolean = true,
    val error: RemoteFailure? = null,
    val loadFailed: Boolean = false,
)

class ReminderEditorViewModel(
    private val loadReminder: LoadReminder,
    private val saveReminder: SaveReminder,
    private val deleteReminder: DeleteReminder,
    private val settings: ConnectionSettingsStore,
    private val savedState: SavedStateHandle,
) : BaseViewModel<ReminderEditorUiState>(restoreState(savedState)) {
    private val restoredDraft: ReminderEditorDraft? = ReminderEditorDraftCodec.fromMap(
        savedState.keys().associateWith { key -> savedState.get<Any?>(key) },
    )

    private val editorCompletion = EditorCompletion(
        savedState.get<String>(KEY_COMPLETION)?.let { raw ->
            ReminderEditorCompletion.entries.firstOrNull { it.name == raw }
        },
    )
    val completion: StateFlow<ReminderEditorCompletion?> = editorCompletion.state

    private val createIdentity = IdempotentRequestKey<CreatePayload>()
    private var started = false

    init {
        val savedKey = savedState.get<String>(KEY_CREATE_KEY)
        val savedEndpoint = savedState.get<String>(KEY_CREATE_ENDPOINT)
        val currentEndpoint = settings.config.value.endpoint?.baseUrl().orEmpty()
        if (restoredDraft != null &&
            restoredDraft.existingId == null &&
            savedKey != null &&
            savedEndpoint == currentEndpoint
        ) {
            createIdentity.adopt(
                CreatePayload(restoredDraft.toReminderDraft(), currentEndpoint),
                savedKey,
            )
        }
    }

    private data class CreatePayload(
        val draft: ReminderDraft,
        val endpoint: String,
    )

    fun start(reminderId: String?) {
        if (started) return
        started = true
        if (restoredDraft != null) return
        if (reminderId == null) {
            persistDraft()
            return
        }
        mutableState.value = mutableState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            when (val result = loadReminder(reminderId)) {
                is Outcome.Success -> {
                    mutableState.value = result.value.toState()
                    persistDraft()
                }
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    loading = false,
                    error = result.failure,
                    loadFailed = true,
                )
            }
        }
    }

    fun setLabel(value: String) = edit { it.copy(label = value) }

    fun setDate(value: LocalDate) = edit { it.copy(date = value) }

    fun setTime(hour: Int, minute: Int) = edit { it.copy(hour = hour, minute = minute) }

    fun setTimezone(value: String) = edit { it.copy(timezone = value) }

    fun setEnabled(value: Boolean) = edit { it.copy(enabled = value) }

    fun save() {
        val current = mutableState.value
        if (current.saving) return
        val draft = current.toReminderDraft()
        val endpoint = settings.config.value.endpoint?.baseUrl().orEmpty()
        val key = if (current.existing == null) {
            createIdentity.keyFor(CreatePayload(draft, endpoint)).also {
                savedState[KEY_CREATE_KEY] = it
                savedState[KEY_CREATE_ENDPOINT] = endpoint
            }
        } else {
            clearPendingKey()
            ""
        }
        mutableState.value = current.copy(saving = true, error = null)
        viewModelScope.launch {
            when (val result = saveReminder(current.existing, draft, key)) {
                is Outcome.Success -> {
                    createIdentity.markSucceeded()
                    clearPendingKey()
                    mutableState.value = mutableState.value.copy(
                        saving = false,
                        existing = result.value,
                        error = null,
                    )
                    persistDraft()
                    setCompletion(ReminderEditorCompletion.SAVED)
                }
                is Outcome.Failure -> {
                    if (!result.failure.isAmbiguousRetryable()) {
                        createIdentity.reset()
                        clearPendingKey()
                    }
                    mutableState.value = mutableState.value.copy(
                        saving = false,
                        error = result.failure,
                    )
                }
            }
        }
    }

    fun delete() {
        val existing = mutableState.value.existing ?: return
        if (mutableState.value.saving) return
        mutableState.value = mutableState.value.copy(saving = true, error = null)
        viewModelScope.launch {
            when (val result = deleteReminder(existing.id)) {
                is Outcome.Success -> {
                    mutableState.value = mutableState.value.copy(saving = false, error = null)
                    setCompletion(ReminderEditorCompletion.DELETED)
                }
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    saving = false,
                    error = result.failure,
                )
            }
        }
    }

    fun consumeCompletion() {
        editorCompletion.consume()
        savedState.remove<String>(KEY_COMPLETION)
    }

    private fun edit(transform: (ReminderEditorUiState) -> ReminderEditorUiState) {
        createIdentity.reset()
        clearPendingKey()
        mutableState.value = transform(mutableState.value)
        persistDraft()
    }

    private fun clearPendingKey() {
        savedState.remove<String>(KEY_CREATE_KEY)
        savedState.remove<String>(KEY_CREATE_ENDPOINT)
    }

    private fun ReminderEditorUiState.toReminderDraft(): ReminderDraft = ReminderDraft(
        label = label,
        date = date,
        hour = hour,
        minute = minute,
        timezone = timezone,
        enabled = enabled,
    )

    private fun ReminderEditorDraft.toReminderDraft(): ReminderDraft = ReminderDraft(
        label = label,
        date = date,
        hour = hour,
        minute = minute,
        timezone = timezone,
        enabled = enabled,
    )

    private fun setCompletion(completion: ReminderEditorCompletion) {
        editorCompletion.publish(completion)
        savedState[KEY_COMPLETION] = completion.name
    }

    private fun persistDraft() {
        val current = mutableState.value
        val draft = ReminderEditorDraft(
            existingId = current.existing?.id,
            existingRevision = current.existing?.revision,
            label = current.label,
            date = current.date,
            hour = current.hour,
            minute = current.minute,
            timezone = current.timezone,
            enabled = current.enabled,
        )
        ReminderEditorDraftCodec.toMap(draft).forEach { (key, value) -> savedState[key] = value }
    }

    private fun Schedule.toState(): ReminderEditorUiState = ReminderEditorUiState(
        loading = false,
        existing = this,
        label = label,
        date = runCatching { LocalDate.parse(onceDate) }.getOrDefault(LocalDate.now()),
        hour = at.hour,
        minute = at.minute,
        timezone = timezone,
        enabled = enabled,
    )

    companion object {
        private const val KEY_COMPLETION = "reminder_completion"
        private const val KEY_CREATE_KEY = "reminder_create_key"
        private const val KEY_CREATE_ENDPOINT = "reminder_create_endpoint"
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class ReminderEditorViewModelFactoryContributor(
    private val loadReminder: LoadReminder,
    private val saveReminder: SaveReminder,
    private val deleteReminder: DeleteReminder,
    private val settings: ConnectionSettingsStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ReminderEditorViewModel::class.java

    override fun createFactory() = SavedStateViewModelFactory { savedState ->
        ReminderEditorViewModel(loadReminder, saveReminder, deleteReminder, settings, savedState)
    }
}

private fun restoreState(savedState: SavedStateHandle): ReminderEditorUiState =
    ReminderEditorDraftCodec.fromMap(savedState.keys().associateWith { key -> savedState.get<Any?>(key) })?.toUiState()
        ?: ReminderEditorUiState()

private fun ReminderEditorDraft.toUiState(): ReminderEditorUiState = ReminderEditorUiState(
    existing = restoredExisting(),
    label = label,
    date = date,
    hour = hour,
    minute = minute,
    timezone = timezone,
    enabled = enabled,
)

private fun ReminderEditorDraft.restoredExisting(): Schedule? {
    val id = existingId ?: return null
    return Schedule(
        id = id,
        label = label,
        kind = ScheduleKind.REMINDER,
        timezone = timezone,
        at = WallTime(hour, minute),
        onceDate = date.toString(),
        repeat = dev.proflyder.home.core.model.RepeatKind.ONCE,
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

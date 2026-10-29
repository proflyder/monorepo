package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.model.isAmbiguousRetryable
import dev.proflyder.home.core.operation.EditorCompletion
import dev.proflyder.home.core.operation.IdempotentRequestKey
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.AlarmEditorDraft
import dev.proflyder.home.feature.alarms.AlarmEditorDraftCodec
import dev.proflyder.home.feature.alarms.domain.DeleteAlarm
import dev.proflyder.home.feature.alarms.domain.LoadAlarm
import dev.proflyder.home.feature.alarms.domain.SaveAlarm
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

data class AlarmEditorUiState(
    val loading: Boolean = false,
    val saving: Boolean = false,
    val existing: Schedule? = null,
    val label: String = "",
    val hour: Int = 7,
    val minute: Int = 0,
    val timezone: String = ZoneId.systemDefault().id,
    val repeat: RepeatKind = RepeatKind.DAILY,
    val weekdays: Set<Int> = emptySet(),
    val enabled: Boolean = true,
    val onceDate: LocalDate? = null,
    val error: RemoteFailure? = null,
    val loadFailed: Boolean = false,
)

class AlarmEditorViewModel(
    private val loadAlarm: LoadAlarm,
    private val saveAlarm: SaveAlarm,
    private val deleteAlarm: DeleteAlarm,
    private val settings: ConnectionSettingsStore,
    private val savedState: SavedStateHandle,
) : BaseViewModel<AlarmEditorUiState>(restoreState(savedState)) {
    private val restoredDraft: AlarmEditorDraft? = AlarmEditorDraftCodec.fromMap(
        savedState.keys().associateWith { key -> savedState.get<Any?>(key) },
    )

    private val editorCompletion = EditorCompletion(
        savedState.get<String>(KEY_COMPLETION)?.let { raw ->
            AlarmEditorCompletion.entries.firstOrNull { it.name == raw }
        },
    )
    val completion: StateFlow<AlarmEditorCompletion?> = editorCompletion.state

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
                CreatePayload(restoredDraft.toAlarmDraft(), currentEndpoint),
                savedKey,
            )
        }
    }

    private data class CreatePayload(
        val draft: AlarmDraft,
        val endpoint: String,
    )

    fun start(alarmId: String?) {
        if (started) return
        started = true
        if (restoredDraft != null) return
        if (alarmId == null) {
            persistDraft()
            return
        }
        mutableState.value = mutableState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            when (val result = loadAlarm(alarmId)) {
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

    fun setTime(hour: Int, minute: Int) = edit { it.copy(hour = hour, minute = minute) }

    fun setTimezone(value: String) = edit { it.copy(timezone = value) }

    fun setRepeat(repeat: RepeatKind) = edit {
        it.copy(
            repeat = repeat,
            onceDate = when {
                repeat == RepeatKind.ONCE -> it.onceDate ?: LocalDate.now(zoneOrSystem(it.timezone))
                else -> null
            },
        )
    }

    fun setWeekday(day: Int, selected: Boolean) = edit {
        val days = it.weekdays.toMutableSet()
        if (selected) days.add(day) else days.remove(day)
        it.copy(weekdays = days)
    }

    fun setEnabled(value: Boolean) = edit { it.copy(enabled = value) }

    fun setDate(value: LocalDate) = edit { it.copy(onceDate = value) }

    fun save() {
        val current = mutableState.value
        if (current.saving) return
        val draft = current.toAlarmDraft()
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
            when (val result = saveAlarm(current.existing, draft, key)) {
                is Outcome.Success -> {
                    createIdentity.markSucceeded()
                    clearPendingKey()
                    mutableState.value = mutableState.value.copy(
                        saving = false,
                        existing = result.value,
                        error = null,
                    )
                    persistDraft()
                    setCompletion(AlarmEditorCompletion.SAVED)
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
            when (val result = deleteAlarm(existing.id)) {
                is Outcome.Success -> {
                    mutableState.value = mutableState.value.copy(saving = false, error = null)
                    setCompletion(AlarmEditorCompletion.DELETED)
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

    private fun edit(transform: (AlarmEditorUiState) -> AlarmEditorUiState) {
        createIdentity.reset()
        clearPendingKey()
        mutableState.value = transform(mutableState.value)
        persistDraft()
    }

    private fun clearPendingKey() {
        savedState.remove<String>(KEY_CREATE_KEY)
        savedState.remove<String>(KEY_CREATE_ENDPOINT)
    }

    private fun AlarmEditorUiState.toAlarmDraft(): AlarmDraft = AlarmDraft(
        label = label,
        hour = hour,
        minute = minute,
        timezone = timezone,
        repeat = repeat,
        weekdays = weekdays.sorted(),
        enabled = enabled,
        onceDate = if (repeat == RepeatKind.ONCE) onceDate else null,
    )

    private fun AlarmEditorDraft.toAlarmDraft(): AlarmDraft = AlarmDraft(
        label = label,
        hour = hour,
        minute = minute,
        timezone = timezone,
        repeat = repeat,
        weekdays = weekdays.sorted(),
        enabled = enabled,
        onceDate = if (repeat == RepeatKind.ONCE) onceDate else null,
    )

    private fun setCompletion(completion: AlarmEditorCompletion) {
        editorCompletion.publish(completion)
        savedState[KEY_COMPLETION] = completion.name
    }

    private fun persistDraft() {
        val current = mutableState.value
        val draft = AlarmEditorDraft(
            existingId = current.existing?.id,
            existingRevision = current.existing?.revision,
            label = current.label,
            hour = current.hour,
            minute = current.minute,
            timezone = current.timezone,
            repeat = current.repeat,
            weekdays = current.weekdays.sorted(),
            enabled = current.enabled,
            onceDate = current.onceDate,
        )
        AlarmEditorDraftCodec.toMap(draft).forEach { (key, value) -> savedState[key] = value }
    }

    private fun Schedule.toState(): AlarmEditorUiState = AlarmEditorUiState(
        loading = false,
        existing = this,
        label = label,
        hour = at.hour,
        minute = at.minute,
        timezone = timezone,
        repeat = repeat,
        weekdays = weekdays.toSet(),
        enabled = enabled,
        onceDate = if (repeat == RepeatKind.ONCE) {
            runCatching { LocalDate.parse(onceDate) }.getOrNull()
        } else {
            null
        },
    )

    private fun zoneOrSystem(timezone: String): ZoneId =
        runCatching { ZoneId.of(timezone) }.getOrDefault(ZoneId.systemDefault())

    companion object {
        private const val KEY_COMPLETION = "alarm_completion"
        private const val KEY_CREATE_KEY = "alarm_create_key"
        private const val KEY_CREATE_ENDPOINT = "alarm_create_endpoint"
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class AlarmEditorViewModelFactoryContributor(
    private val loadAlarm: LoadAlarm,
    private val saveAlarm: SaveAlarm,
    private val deleteAlarm: DeleteAlarm,
    private val settings: ConnectionSettingsStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = AlarmEditorViewModel::class.java

    override fun createFactory() = SavedStateViewModelFactory { savedState ->
        AlarmEditorViewModel(loadAlarm, saveAlarm, deleteAlarm, settings, savedState)
    }
}

private fun restoreState(savedState: SavedStateHandle): AlarmEditorUiState =
    AlarmEditorDraftCodec.fromMap(savedState.keys().associateWith { key -> savedState.get<Any?>(key) })?.toUiState()
        ?: AlarmEditorUiState()

private fun AlarmEditorDraft.toUiState(): AlarmEditorUiState = AlarmEditorUiState(
    existing = restoredExisting(),
    label = label,
    hour = hour,
    minute = minute,
    timezone = timezone,
    repeat = repeat,
    weekdays = weekdays.toSet(),
    enabled = enabled,
    onceDate = onceDate,
)

private fun AlarmEditorDraft.restoredExisting(): Schedule? {
    val id = existingId ?: return null
    return Schedule(
        id = id,
        label = label,
        kind = ScheduleKind.ALARM,
        timezone = timezone,
        at = WallTime(hour, minute),
        onceDate = if (repeat == RepeatKind.ONCE) onceDate?.toString().orEmpty() else "",
        repeat = repeat,
        weekdays = weekdays,
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

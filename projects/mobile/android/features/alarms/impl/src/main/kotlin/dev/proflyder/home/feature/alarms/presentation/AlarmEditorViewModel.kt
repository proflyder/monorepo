package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.navigation.destination
import android.content.Context
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.presentation.viewModel
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.take
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.presentation.EditorDraftCodec
import dev.proflyder.home.core.presentation.EditorDraftSession
import dev.proflyder.home.core.presentation.EditorStatus
import dev.proflyder.home.core.presentation.RouteNotice
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.AlarmEditorDraft
import dev.proflyder.home.feature.alarms.AlarmEditorDraftCodec
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.defaultAlarmTime
import dev.proflyder.home.feature.alarms.di.AlarmsFeatureScope
import dev.proflyder.home.feature.alarms.di.AlarmsGraph
import dev.proflyder.home.feature.alarms.domain.SaveAlarm
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.quickOffsetTime
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.time.LocalDate
import java.time.LocalDateTime

internal data class AlarmEditorUiState(
    val existing: Schedule? = null,
    val label: String = "",
    val hour: Int = defaultAlarmTime(LocalDateTime.now()).hour,
    val minute: Int = defaultAlarmTime(LocalDateTime.now()).minute,
    val repeat: RepeatKind = RepeatKind.ONCE,
    val weekdays: Set<Int> = emptySet(),
    val enabled: Boolean = true,
    val onceDate: LocalDate? = LocalDate.now(),
    val status: EditorStatus = EditorStatus(),
    val editing: Boolean = false,
)

internal sealed interface AlarmEditorIntent {
    data class SetLabel(val value: String) : AlarmEditorIntent
    data class SetTime(val hour: Int, val minute: Int) : AlarmEditorIntent
    data class QuickOffset(val minutes: Int) : AlarmEditorIntent
    data class SetRepeat(val repeat: RepeatKind, val days: Set<Int>) : AlarmEditorIntent
    data class SetEnabled(val value: Boolean) : AlarmEditorIntent
    data class SetDate(val value: LocalDate) : AlarmEditorIntent
    data object PickTime : AlarmEditorIntent
    data object PickDate : AlarmEditorIntent
    data object PickRepeat : AlarmEditorIntent
    data object Save : AlarmEditorIntent
    data object CompletionShown : AlarmEditorIntent
}

internal sealed interface AlarmEditorEvent {
    data class PickTime(val hour: Int, val minute: Int) : AlarmEditorEvent
    data class PickDate(val selected: LocalDate) : AlarmEditorEvent
    data class PickRepeat(val repeat: RepeatKind, val days: Set<Int>) : AlarmEditorEvent
    data class Completed(val notice: RouteNotice) : AlarmEditorEvent
    data class OpenRoute(val route: Route) : AlarmEditorEvent
}

internal object AlarmDraftCodec : EditorDraftCodec<AlarmEditorDraft> {
    override fun encode(draft: AlarmEditorDraft): Map<String, Any?> = AlarmEditorDraftCodec.toMap(draft)

    override fun decode(values: Map<String, Any?>): AlarmEditorDraft? = AlarmEditorDraftCodec.fromMap(values)
}

internal typealias AlarmDraftSession = EditorDraftSession<AlarmEditorDraft, AlarmDraft, AlarmEditorCompletion>

internal class AlarmEditorViewModel(
    private val repository: AlarmRepository,
    private val saveAlarm: SaveAlarm,
    private val session: AlarmDraftSession,
    private val defaultLabel: String,
    alarmId: String?,
) : StateViewModel<AlarmEditorUiState, AlarmEditorIntent, AlarmEditorEvent>(
    session.restored?.toUiState() ?: AlarmEditorUiState(editing = alarmId != null),
) {
    init {
        session.restored?.takeIf { it.existingId == null }?.let { session.adoptRestoredCreate(it.toAlarmDraft()) }
        session.pendingCompletion?.let { emit(AlarmEditorEvent.Completed(noticeFor(it, created = null))) }
        start(alarmId)
    }

    override fun handle(intent: AlarmEditorIntent) {
        when (intent) {
            is AlarmEditorIntent.SetLabel -> edit { copy(label = intent.value) }
            is AlarmEditorIntent.SetTime -> edit { copy(hour = intent.hour, minute = intent.minute) }
            is AlarmEditorIntent.QuickOffset -> edit { withQuickOffset(LocalDateTime.now(), intent.minutes) }
            is AlarmEditorIntent.SetRepeat -> edit { withRepeat(intent.repeat, intent.days) }
            is AlarmEditorIntent.SetEnabled -> edit { copy(enabled = intent.value) }
            is AlarmEditorIntent.SetDate -> edit { copy(onceDate = intent.value) }
            AlarmEditorIntent.Save -> save()
            AlarmEditorIntent.CompletionShown -> {
                session.consumeCompletion()
                emit(AlarmEditorEvent.OpenRoute(Route.Back))
            }
            else -> emitPicker(intent)
        }
    }

    private fun emitPicker(intent: AlarmEditorIntent) {
        val state = currentState
        when (intent) {
            AlarmEditorIntent.PickTime -> emit(AlarmEditorEvent.PickTime(state.hour, state.minute))
            AlarmEditorIntent.PickDate -> emit(AlarmEditorEvent.PickDate(state.onceDate ?: LocalDate.now()))
            AlarmEditorIntent.PickRepeat -> emit(AlarmEditorEvent.PickRepeat(state.repeat, state.weekdays))
            else -> Unit
        }
    }

    private fun start(alarmId: String?) {
        if (session.restored != null) return
        if (alarmId == null) return session.persist(currentState.toEditorDraft())
        closeWhenRemoved(alarmId)
        repository[alarmId]?.let(::loaded) ?: fetch(alarmId)
    }

    private fun fetch(alarmId: String) {
        updateState { copy(status = status.loadingStarted()) }
        launch {
            when (val result = repository.load(alarmId)) {
                is Outcome.Success -> loaded(result.value)
                is Outcome.Failure -> updateState { copy(status = status.loadFailed(result.failure)) }
            }
        }
    }

    private fun closeWhenRemoved(alarmId: String) {
        repository.observe(alarmId)
            .dropWhile { it == null }
            .filter { it == null && !currentState.status.saving }
            .take(1)
            .onEach { emit(AlarmEditorEvent.OpenRoute(Route.Back)) }
            .launchIn(viewModelScope)
    }

    private fun loaded(alarm: Schedule) {
        updateState { alarm.toUiState() }
        session.persist(currentState.toEditorDraft())
    }

    private fun edit(transform: AlarmEditorUiState.() -> AlarmEditorUiState) {
        updateState(transform)
        session.edited(currentState.toEditorDraft())
    }

    private fun save() {
        val current = currentState
        if (current.status.saving) return
        val draft = current.toAlarmDraft()
        updateState { copy(status = status.savingStarted()) }
        launch { submit(current.existing, draft) }
    }

    private suspend fun submit(existing: Schedule?, draft: AlarmDraft) {
        val result = session.submit(draft.takeIf { existing == null }) { key -> saveAlarm(existing, draft, key, defaultLabel) }
        val created = (result as? Outcome.Success)?.value?.id?.takeIf { existing == null }
        if (result is Outcome.Success) updateState { copy(existing = result.value) }
        finish(result, AlarmEditorCompletion.SAVED, created)
    }

    private fun finish(result: Outcome<*>, completion: AlarmEditorCompletion, created: String? = null) {
        val failure = (result as? Outcome.Failure)?.failure
        updateState { copy(status = status.settled(failure)) }
        if (failure != null) return
        session.persist(currentState.toEditorDraft())
        session.complete(completion)
        emit(AlarmEditorEvent.Completed(noticeFor(completion, created)))
    }

    private fun noticeFor(completion: AlarmEditorCompletion, created: String?): RouteNotice = when (completion) {
        AlarmEditorCompletion.SAVED -> RouteNotice(R.string.alarms_saved, created?.let { Route.Screen(AlarmEditorDestination(it)) })
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<SavedStateViewModelFactory>())
@ClassKey(AlarmEditorViewModel::class)
@Inject
class AlarmEditorViewModelSource internal constructor(
    private val scope: AlarmsFeatureScope,
    private val settings: ConnectionSettingsStore,
    private val context: Context,
) : SavedStateViewModelFactory {
    override fun create(savedState: SavedStateHandle): ViewModel = scope.viewModel { graph -> build(graph, savedState) }

    private fun build(graph: AlarmsGraph, savedState: SavedStateHandle) = AlarmEditorViewModel(
        repository = graph.repository,
        saveAlarm = graph.saveAlarm,
        session = EditorDraftSession(savedState, KEY_PREFIX, AlarmDraftCodec, AlarmEditorCompletion.entries) {
            settings.config.value.endpoint?.baseUrl().orEmpty()
        },
        defaultLabel = context.getString(R.string.alarms_label_default),
        alarmId = savedState.destination(AlarmEditorDestination)?.alarmId,
    )

    private companion object {
        const val KEY_PREFIX = "alarm"
    }
}

internal fun AlarmEditorUiState.withQuickOffset(now: LocalDateTime, offsetMinutes: Int): AlarmEditorUiState {
    val time = quickOffsetTime(now, offsetMinutes)
    return copy(hour = time.hour, minute = time.minute)
}

internal fun AlarmEditorUiState.withRepeat(repeat: RepeatKind, days: Set<Int>): AlarmEditorUiState {
    val normalized = if (repeat == RepeatKind.CUSTOM) AlarmRepeatRules.presetFor(days) else repeat
    val chosenDays = when (normalized) {
        RepeatKind.CUSTOM -> days
        else -> AlarmRepeatRules.daysForPreset(normalized).ifEmpty { weekdays }
    }
    return copy(
        repeat = normalized,
        weekdays = chosenDays,
        onceDate = if (normalized == RepeatKind.ONCE) onceDate ?: LocalDate.now() else null,
    )
}

internal fun AlarmEditorUiState.toAlarmDraft(): AlarmDraft = AlarmDraft(
    label = label,
    hour = hour,
    minute = minute,
    repeat = repeat,
    weekdays = weekdays.sorted(),
    enabled = enabled,
    onceDate = if (repeat == RepeatKind.ONCE) onceDate else null,
)

internal fun AlarmEditorUiState.toEditorDraft(): AlarmEditorDraft = AlarmEditorDraft(
    existingId = existing?.id,
    existingRevision = existing?.revision,
    label = label,
    hour = hour,
    minute = minute,
    repeat = repeat,
    weekdays = weekdays.sorted(),
    enabled = enabled,
    onceDate = onceDate,
)

internal fun AlarmEditorDraft.toAlarmDraft(): AlarmDraft = toUiState().toAlarmDraft()

internal fun Schedule.toUiState(): AlarmEditorUiState = AlarmEditorUiState(
    existing = this,
    label = label,
    hour = at.hour,
    minute = at.minute,
    repeat = repeat,
    weekdays = weekdays.toSet(),
    enabled = enabled,
    onceDate = if (repeat == RepeatKind.ONCE) runCatching { LocalDate.parse(onceDate) }.getOrNull() else null,
    editing = true,
)

internal fun AlarmEditorDraft.toUiState(): AlarmEditorUiState = AlarmEditorUiState(
    existing = restoredExisting(),
    label = label,
    hour = hour,
    minute = minute,
    repeat = repeat,
    weekdays = weekdays.toSet(),
    enabled = enabled,
    onceDate = onceDate,
    editing = existingId != null,
)

internal fun AlarmEditorDraft.restoredExisting(): Schedule? {
    val id = existingId ?: return null
    return Schedule(
        id = id,
        label = label,
        kind = ScheduleKind.ALARM,
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

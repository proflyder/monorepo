package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.DeliveredWhenStarted
import dev.proflyder.home.core.presentation.ExclusiveActions
import dev.proflyder.home.core.presentation.FlowKey
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.UndoWindow
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.alarms.AlarmDisableChoice
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.alarmDisableChoice
import dev.proflyder.home.feature.alarms.di.AlarmsFeatureScope
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.toAlarmDraft
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal object AlarmsFlow : FlowKey("alarms")

internal sealed interface AlarmActionIntent {
    data class Toggle(val id: String, val enabled: Boolean) : AlarmActionIntent
    data class SkipOnce(val id: String) : AlarmActionIntent
    data class TurnOff(val id: String) : AlarmActionIntent
    data class Snooze(val id: String) : AlarmActionIntent
    data class Stop(val id: String) : AlarmActionIntent
    data class RequestDelete(val id: String) : AlarmActionIntent
    data class Delete(val id: String) : AlarmActionIntent
    data object Undo : AlarmActionIntent
}

internal sealed interface AlarmActionEvent {
    data class AskDisable(val id: String) : AlarmActionEvent
    data class ConfirmDelete(val id: String) : AlarmActionEvent
    data class Failure(val failure: RemoteFailure) : AlarmActionEvent, DeliveredWhenStarted
}

internal class AlarmActions(
    private val repository: AlarmRepository,
    private val snoozeAlarm: SnoozeAlarm,
    scope: CoroutineScope,
    private val emit: (AlarmActionEvent) -> Unit,
    onBusy: (Boolean) -> Unit,
    private val undo: UndoWindow<Schedule>? = null,
) {
    private val runner = ExclusiveActions(scope, { key -> onBusy(key != null) }) { emit(AlarmActionEvent.Failure(it)) }

    fun handle(intent: AlarmActionIntent) {
        when (intent) {
            is AlarmActionIntent.Toggle -> toggle(intent.id, intent.enabled)
            is AlarmActionIntent.SkipOnce -> run(intent.id) { repository.skipNextOccurrence(it) }
            is AlarmActionIntent.TurnOff -> run(intent.id) { repository.setEnabled(it, false) }
            is AlarmActionIntent.Snooze -> run(intent.id) { snoozeAlarm(it) }
            is AlarmActionIntent.Stop -> run(intent.id) { repository.stop(it) }
            is AlarmActionIntent.RequestDelete -> emit(AlarmActionEvent.ConfirmDelete(intent.id))
            is AlarmActionIntent.Delete -> delete(intent.id)
            AlarmActionIntent.Undo -> restore()
        }
    }

    private fun toggle(id: String, enabled: Boolean) {
        val alarm = repository[id] ?: return
        when {
            enabled -> run(id) { repository.setEnabled(it, true) }
            alarmDisableChoice(alarm) == AlarmDisableChoice.DISABLE -> run(id) { repository.setEnabled(it, false) }
            else -> emit(AlarmActionEvent.AskDisable(id))
        }
    }

    private fun delete(id: String) {
        val alarm = repository[id] ?: return
        runner.run(id, { repository.delete(id) }) { undo?.open(alarm) }
    }

    private fun restore() {
        val alarm = undo?.take() ?: return
        runner.run(RESTORE, { repository.create(alarm.toAlarmDraft(), UUID.randomUUID().toString()) }) { }
    }

    private fun run(id: String, action: suspend (String) -> Outcome<Schedule>) {
        runner.run(id, { action(id) }) { }
    }

    private companion object {
        const val RESTORE = "restore"
    }
}

internal data class AlarmActionsState(
    val busy: Boolean = false,
    val undo: Schedule? = null,
)

internal class AlarmsActionsViewModel(
    repository: AlarmRepository,
    snoozeAlarm: SnoozeAlarm,
) : StateViewModel<AlarmActionsState, AlarmActionIntent, AlarmActionEvent>(AlarmActionsState()) {
    private val window = UndoWindow<Schedule>(viewModelScope)
    private val actions = AlarmActions(repository, snoozeAlarm, viewModelScope, ::emit, ::onBusy, window)

    init {
        window.pending.onEach { updateState { copy(undo = it) } }.launchIn(viewModelScope)
    }

    override fun handle(intent: AlarmActionIntent) = actions.handle(intent)

    private fun onBusy(busy: Boolean) = updateState { copy(busy = busy) }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AlarmsActionsViewModel::class)
@Inject
class AlarmsActionsViewModelSource internal constructor(
    private val scope: AlarmsFeatureScope,
) : ViewModelSource {
    override fun create(): ViewModel = scope.viewModel { AlarmsActionsViewModel(it.repository, it.snoozeAlarm) }
}

internal class AlarmPrompts(
    private val fragment: BaseFragment<*>,
    private val dispatch: (AlarmActionIntent) -> Unit,
) {
    fun show(event: AlarmActionEvent) {
        when (event) {
            is AlarmActionEvent.AskDisable -> askDisable(event.id)
            is AlarmActionEvent.ConfirmDelete -> confirmDelete(event.id)
            is AlarmActionEvent.Failure -> fragment.showFailure(event.failure)
        }
    }

    private fun askDisable(id: String) {
        MaterialAlertDialogBuilder(fragment.requireContext())
            .setTitle(R.string.alarms_disable_title)
            .setMessage(R.string.alarms_disable_message)
            .setNeutralButton(R.string.alarms_disable_skip_once) { _, _ -> dispatch(AlarmActionIntent.SkipOnce(id)) }
            .setPositiveButton(R.string.alarms_disable_turn_off) { _, _ -> dispatch(AlarmActionIntent.TurnOff(id)) }
            .setNegativeButton(DesignR.string.ds_action_cancel, null)
            .show()
    }

    private fun confirmDelete(id: String) {
        fragment.confirm(R.string.alarms_delete_confirm, DesignR.string.ds_action_delete, DesignR.string.ds_action_cancel) {
            dispatch(AlarmActionIntent.Delete(id))
        }
    }
}

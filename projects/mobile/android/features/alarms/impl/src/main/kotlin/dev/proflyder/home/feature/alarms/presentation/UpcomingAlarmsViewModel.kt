package dev.proflyder.home.feature.alarms.presentation

import androidx.fragment.app.Fragment
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScreenSectionKey
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.ScreenSection
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.UpcomingAlarmsSection
import dev.proflyder.home.feature.alarms.di.AlarmsFeatureScope
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class UpcomingAlarm(val alarm: Schedule, val ringing: Boolean)

internal data class UpcomingAlarmsState(
    val alarms: List<UpcomingAlarm> = emptyList(),
    val nowUnix: Long = 0,
)

internal sealed interface UpcomingAlarmsIntent {
    data class Open(val id: String) : UpcomingAlarmsIntent
    data class Act(val action: AlarmActionIntent) : UpcomingAlarmsIntent
}

internal sealed interface UpcomingAlarmsEvent {
    data class Action(val event: AlarmActionEvent) : UpcomingAlarmsEvent
    data class OpenRoute(val route: Route) : UpcomingAlarmsEvent
}

internal object UpcomingAlarmRules {
    const val MAX_ROWS = 3

    fun isRinging(alarm: Schedule, nowUnix: Long): Boolean =
        alarm.activeOccurrenceUnix > 0 && (alarm.activeUntilUnix <= 0 || alarm.activeUntilUnix > nowUnix)

    fun upcoming(alarms: List<Schedule>, nowUnix: Long): List<UpcomingAlarm> = alarms
        .map { UpcomingAlarm(it, isRinging(it, nowUnix)) }
        .filter { it.ringing || (it.alarm.enabled && it.alarm.nextDueUnix > 0) }
        .sortedWith(compareBy({ if (it.ringing) nowUnix else it.alarm.nextDueUnix }, { it.alarm.id }))
        .take(MAX_ROWS)
}

internal class UpcomingAlarmsViewModel(
    private val repository: AlarmRepository,
    snoozeAlarm: SnoozeAlarm,
    private val clock: ServerClock,
) : StateViewModel<UpcomingAlarmsState, UpcomingAlarmsIntent, UpcomingAlarmsEvent>(UpcomingAlarmsState()) {
    private val actions = AlarmActions(repository, snoozeAlarm, viewModelScope, { emit(UpcomingAlarmsEvent.Action(it)) }, { })

    init {
        repository.items.onEach(::onAlarms).launchIn(viewModelScope)
        repository.refreshOn(visibility.returned)
    }

    override fun handle(intent: UpcomingAlarmsIntent) {
        when (intent) {
            is UpcomingAlarmsIntent.Open -> open(intent.id)
            is UpcomingAlarmsIntent.Act -> actions.handle(intent.action)
        }
    }

    private fun onAlarms(alarms: List<Schedule>) {
        val now = clock.nowUnix()
        updateState { copy(alarms = UpcomingAlarmRules.upcoming(alarms, now), nowUnix = now) }
    }

    private fun open(id: String) {
        val ringing = currentState.alarms.firstOrNull { it.alarm.id == id }?.ringing == true
        if (ringing) return actions.handle(AlarmActionIntent.Stop(id))
        emit(UpcomingAlarmsEvent.OpenRoute(Route.Screen(AlarmEditorDestination(id))))
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(UpcomingAlarmsViewModel::class)
@Inject
class UpcomingAlarmsViewModelSource internal constructor(
    private val scope: AlarmsFeatureScope,
    private val clock: ServerClock,
) : ViewModelSource {
    override fun create(): ViewModel = scope.viewModel { UpcomingAlarmsViewModel(it.repository, it.snoozeAlarm, clock) }
}

@ContributesIntoSet(AppScope::class, binding = binding<ScreenSection>())
@Inject
class UpcomingAlarmsScreenSection internal constructor() : ScreenSection {
    override val key: ScreenSectionKey = UpcomingAlarmsSection

    override fun create(): Fragment = UpcomingAlarmsFragment()
}

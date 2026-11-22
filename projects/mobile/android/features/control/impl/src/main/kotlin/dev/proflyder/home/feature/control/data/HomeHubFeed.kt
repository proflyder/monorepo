package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.timers.TimerRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.stateIn

internal data class HubData(
    val alarms: List<Schedule>? = null,
    val timers: List<Timer>? = null,
    val reminders: List<Schedule>? = null,
    val scenarios: List<Scenario>? = null,
) {
    fun keepingKnown(fresh: HubData): HubData = HubData(
        alarms = fresh.alarms ?: alarms,
        timers = fresh.timers ?: timers,
        reminders = fresh.reminders ?: reminders,
        scenarios = fresh.scenarios ?: scenarios,
    )
}

internal class HubSources(
    val alarms: AlarmRepository,
    val timers: TimerRepository,
    val reminders: ReminderRepository,
    val scenarios: ScenarioRepository,
    val clockSync: ServerClockSync,
    val foreground: AppForeground,
)

internal class HomeHubFeed(
    private val sources: HubSources,
    scope: CoroutineScope,
    policy: ControlRefreshPolicy = ControlRefreshPolicy(),
) {
    private val triggers = RefreshTriggers(sources.foreground, policy.period)

    val state: StateFlow<HubData> = triggers.ticks
        .map { load() }
        .runningFold(HubData(), HubData::keepingKnown)
        .stateIn(scope, SharingStarted.WhileSubscribed(policy.stopTimeout.inWholeMilliseconds), HubData())

    fun refresh() = triggers.request()

    private suspend fun load(): HubData {
        sources.clockSync.sync()
        return HubData(
            alarms = sources.alarms.list().getOrNull(),
            timers = sources.timers.list().getOrNull(),
            reminders = sources.reminders.list().getOrNull(),
            scenarios = sources.scenarios.list().getOrNull(),
        )
    }
}

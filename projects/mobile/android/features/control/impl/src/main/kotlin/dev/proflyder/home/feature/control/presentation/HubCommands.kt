package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.feature.alarms.AlarmDisableChoice
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.alarmDisableChoice
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.data.HomeHubFeed
import dev.proflyder.home.feature.timers.TimerRepository

internal class HubCommands(
    private val alarms: AlarmRepository,
    private val timers: TimerRepository,
    private val feed: HomeHubFeed,
) {
    private val inFlight = PendingActions()

    suspend fun perform(command: HubCommand): ControlEvent? = when (command) {
        is HubCommand.Primary -> primary(command.card)
        is HubCommand.Switch -> if (command.enabled) null else primary(command.card)
        is HubCommand.Disable -> call(command.card) { alarms.setEnabled(command.card.schedule.id, false) }
        is HubCommand.Skip -> call(command.card) { alarms.skipNextOccurrence(command.card.schedule.id) }
    }

    private suspend fun primary(card: HubCard): ControlEvent? = when (card) {
        is HubCard.Alarm -> alarmPrimary(card)
        is HubCard.TimerCard -> call(card) { toggleTimer(card) }
        is HubCard.Reminder -> null
    }

    private suspend fun alarmPrimary(card: HubCard.Alarm): ControlEvent? {
        val alarm = card.schedule
        return when {
            card.ringing -> call(card) { alarms.stop(alarm.id) }
            alarm.enabled && alarmDisableChoice(alarm) == AlarmDisableChoice.ASK -> ControlEvent.AskAlarmDisable(card)
            else -> call(card) { alarms.setEnabled(alarm.id, !alarm.enabled) }
        }
    }

    private suspend fun toggleTimer(card: HubCard.TimerCard): Outcome<*> = when (card.timer.state) {
        TimerState.PAUSED -> timers.resume(card.timer.id)
        else -> timers.pause(card.timer.id)
    }

    private suspend fun call(card: HubCard, action: suspend () -> Outcome<*>): ControlEvent? =
        inFlight.run(card.key, action).report { refreshed() }

    private fun refreshed(): ControlEvent? {
        feed.refresh()
        return null
    }
}

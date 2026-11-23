package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.timers.TimerRepository

internal class HubCommands(
    private val timers: TimerRepository,
) {
    private val inFlight = PendingActions()

    suspend fun perform(command: HubCommand): ControlEvent? = when (command) {
        is HubCommand.Primary -> primary(command.card)
    }

    private suspend fun primary(card: HubCard): ControlEvent? = when (card) {
        is HubCard.TimerCard -> call(card) { toggleTimer(card) }
        is HubCard.Reminder -> null
    }

    private suspend fun toggleTimer(card: HubCard.TimerCard): Outcome<*> = when (card.timer.state) {
        TimerState.PAUSED -> timers.resume(card.timer.id)
        else -> timers.pause(card.timer.id)
    }

    private suspend fun call(card: HubCard, action: suspend () -> Outcome<*>): ControlEvent? =
        inFlight.run(card.key, action).report { null }
}

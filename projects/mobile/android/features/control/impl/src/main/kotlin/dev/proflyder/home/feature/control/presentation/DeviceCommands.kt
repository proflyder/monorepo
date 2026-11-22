package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.feature.control.ControlRepository
import dev.proflyder.home.feature.control.SpeakText
import dev.proflyder.home.feature.control.WakeFeedbackAction
import dev.proflyder.home.feature.control.domain.AdjustLight
import dev.proflyder.home.feature.control.domain.PressControl
import dev.proflyder.home.feature.control.domain.RunScenario
import dev.proflyder.home.feature.control.domain.SetDevicePower
import dev.proflyder.home.feature.control.domain.SetVolume
import dev.proflyder.home.feature.control.domain.SetVolumeMuted
import dev.proflyder.home.feature.control.domain.SpeakOnStation
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import java.util.UUID
import kotlinx.coroutines.flow.StateFlow

internal class DeviceCommands(
    repository: ControlRepository,
    private val scenarios: ScenarioRepository,
) {
    private val runEntry = RunScenario(repository)
    private val pressControl = PressControl(repository)
    private val setPower = SetDevicePower(repository)
    private val adjustLight = AdjustLight(repository)
    private val speakOnStation = SpeakOnStation(repository)
    private val setVolume = SetVolume(repository)
    private val setMuted = SetVolumeMuted(repository)

    private val entryActions = PendingActions()
    private val scenarioActions = PendingActions()

    val pendingEntries: StateFlow<Set<String>> = entryActions.pending

    val pendingScenarios: StateFlow<Set<String>> = scenarioActions.pending

    suspend fun perform(command: DeviceCommand): ControlEvent? = when (command) {
        is DeviceCommand.Run -> tracked(command.entry) { runEntry(it) }.report { queued(command.entry.title) }
        is DeviceCommand.Wake -> wake(command.action)
        is DeviceCommand.Press -> tracked(command.entry) { pressControl(it) }.report { sent() }
        is DeviceCommand.Speak -> speak(command.entry, command.text)
        is DeviceCommand.Power -> direct(command.entry) { setPower(it, command.on) }
        is DeviceCommand.Adjust -> direct(command.entry.takeIf { it.light != null }) { adjustLight(it, command.adjust) }
        is DeviceCommand.Volume -> direct(command.entry.takeIf { it.volume != null }) { setVolume(it, command.level) }
        is DeviceCommand.Mute -> direct(command.entry.takeIf { it.volume != null }) { setMuted(it, command.muted) }
        is DeviceCommand.RunScenario -> runScenario(command.scenario)
    }

    private suspend fun wake(action: WakeFeedbackAction): ControlEvent? {
        if (action.busy) return null
        return tracked(action.entry) { runEntry(it) }
            .report { ControlEvent.Notice(ControlNotice.WakeMarked(action.kind)) }
    }

    private suspend fun speak(entry: ControlEntry, text: String): ControlEvent? {
        val message = SpeakText.normalize(text) ?: return null
        return tracked(entry) { speakOnStation(it, message) }.report { queued(entry.title) }
    }

    private suspend fun runScenario(scenario: Scenario): ControlEvent? =
        scenarioActions.run(scenario.id) { scenarios.run(scenario.id, UUID.randomUUID().toString()) }
            .report { queued(scenario.label) }

    private suspend fun tracked(
        entry: ControlEntry,
        action: suspend (ControlEntry) -> Outcome<Unit>,
    ): Outcome<*>? = entry.takeIf { it.available }?.let { target -> entryActions.run(target.id) { action(target) } }

    private suspend fun direct(
        entry: ControlEntry?,
        action: suspend (ControlEntry) -> Outcome<Unit>,
    ): ControlEvent? = entry?.takeIf { it.available }?.let { action(it) }.report { null }

    private fun queued(title: String): ControlEvent = ControlEvent.Notice(ControlNotice.ScenarioQueued(title))

    private fun sent(): ControlEvent = ControlEvent.Notice(ControlNotice.CommandSent)
}

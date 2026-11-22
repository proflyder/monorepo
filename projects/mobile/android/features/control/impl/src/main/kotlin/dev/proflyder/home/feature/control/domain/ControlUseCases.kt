package dev.proflyder.home.feature.control.domain

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.control.ControlRepository

internal class RunScenario(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry): Outcome<Unit> =
        repository.send(entry.id, ControlAction.RUN)
}

internal class PressControl(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry): Outcome<Unit> =
        repository.send(entry.id, ControlAction.PRESS)
}

internal class SetDevicePower(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, on: Boolean): Outcome<Unit> =
        repository.send(entry.id, if (on) ControlAction.TURN_ON else ControlAction.TURN_OFF)
}

internal class AdjustLight(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, adjust: LightAdjust): Outcome<Unit> =
        repository.adjust(entry.id, adjust)
}

internal class SpeakOnStation(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, text: String): Outcome<Unit> =
        repository.speak(entry.id, text)
}

internal class SetVolume(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, level: Int): Outcome<Unit> =
        repository.setVolume(entry.id, level)
}

internal class SetVolumeMuted(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, muted: Boolean): Outcome<Unit> =
        repository.setVolumeMuted(entry.id, muted)
}

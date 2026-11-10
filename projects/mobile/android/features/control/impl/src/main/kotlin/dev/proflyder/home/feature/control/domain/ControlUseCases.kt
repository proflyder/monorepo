package dev.proflyder.home.feature.control.domain

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.control.ControlRepository
import dev.zacsweers.metro.Inject

/** Loads the catalog; entries of kinds this app version cannot present are dropped. */
@Inject class LoadControlCatalog(
    private val repository: ControlRepository,
) {
    val isConfigured: Boolean get() = repository.isConfigured

    suspend operator fun invoke(): Outcome<ControlCatalog> = when (val result = repository.catalog()) {
        is Outcome.Success -> Outcome.Success(
            result.value.copy(entries = result.value.entries.filter { it.kind != ControlKind.OTHER }),
        )
        is Outcome.Failure -> result
    }
}

@Inject class RunScenario(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry): Outcome<Unit> =
        repository.send(entry.id, ControlAction.RUN)
}

/** Presses a stateless button such as "lock" or "unlock" the PC. */
@Inject class PressControl(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry): Outcome<Unit> =
        repository.send(entry.id, ControlAction.PRESS)
}

@Inject class SetDevicePower(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, on: Boolean): Outcome<Unit> =
        repository.send(entry.id, if (on) ControlAction.TURN_ON else ControlAction.TURN_OFF)
}

/**
 * Switches the assistant profile. `on` means the named (work) profile, `off`
 * the default (personal) one — the profile entity is a plain switch in Home
 * Assistant, so this is the same command as a device toggle.
 */
@Inject class SetVoiceProfile(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, work: Boolean): Outcome<Unit> =
        repository.send(entry.id, if (work) ControlAction.TURN_ON else ControlAction.TURN_OFF)
}

@Inject class AdjustLight(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, adjust: LightAdjust): Outcome<Unit> =
        repository.adjust(entry.id, adjust)
}

/**
 * Speaks a text on the station. The text is already normalized and validated
 * by [dev.proflyder.home.feature.control.SpeakText]; the server re-validates it.
 */
@Inject class SpeakOnStation(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, text: String): Outcome<Unit> =
        repository.speak(entry.id, text)
}

/** Sets the station volume or its announcement level. */
@Inject class SetVolume(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, level: Int): Outcome<Unit> =
        repository.setVolume(entry.id, level)
}

/** Silences or restores the station media player. */
@Inject class SetVolumeMuted(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, muted: Boolean): Outcome<Unit> =
        repository.setVolumeMuted(entry.id, muted)
}

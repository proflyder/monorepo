package dev.proflyder.home.feature.control.domain

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.control.ControlRepository
import dev.zacsweers.metro.Inject

/** Loads the catalog; entries of kinds this app version cannot present are dropped. */
class LoadControlCatalog @Inject constructor(
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

class RunScenario @Inject constructor(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry): Outcome<Unit> =
        repository.send(entry.id, ControlAction.RUN)
}

class SetDevicePower @Inject constructor(
    private val repository: ControlRepository,
) {
    suspend operator fun invoke(entry: ControlEntry, on: Boolean): Outcome<Unit> =
        repository.send(entry.id, if (on) ControlAction.TURN_ON else ControlAction.TURN_OFF)
}

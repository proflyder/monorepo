package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.ControlRepository
import dev.proflyder.home.feature.control.VoiceProfileState
import dev.proflyder.home.feature.control.VoiceProfileSwitch
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

internal class CatalogVoiceProfileSwitch(
    private val repository: ControlRepository,
) : VoiceProfileSwitch {
    override val profile: Flow<VoiceProfileState?> = repository.catalog
        .map { state -> state.entries.firstOrNull { it.isProfile } }
        .map { entry -> entry?.let { VoiceProfileState(work = it.power == PowerState.ON, available = it.available) } }
        .distinctUntilChanged()

    override suspend fun setWork(work: Boolean): Outcome<Unit> {
        val entry = repository.catalog.value.entries.firstOrNull { it.isProfile } ?: return Outcome.Success(Unit)
        return repository.send(entry.id, if (work) ControlAction.TURN_ON else ControlAction.TURN_OFF)
    }
}

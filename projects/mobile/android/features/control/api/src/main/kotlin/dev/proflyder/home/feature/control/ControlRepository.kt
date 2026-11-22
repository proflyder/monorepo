package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

data class ControlCatalogState(
    val configured: Boolean = true,
    val loading: Boolean = true,
    val catalog: ControlCatalog? = null,
    val failure: RemoteFailure? = null,
) {
    val entries: List<ControlEntry> get() = catalog?.entries.orEmpty()

    val statesOk: Boolean get() = catalog?.statesOk ?: true
}

interface ControlRepository {
    val catalog: StateFlow<ControlCatalogState>

    fun refresh()

    suspend fun send(entryId: String, action: ControlAction): Outcome<Unit>

    suspend fun adjust(entryId: String, adjust: LightAdjust): Outcome<Unit>

    suspend fun speak(entryId: String, text: String): Outcome<Unit>

    suspend fun setVolume(entryId: String, level: Int): Outcome<Unit>

    suspend fun setVolumeMuted(entryId: String, muted: Boolean): Outcome<Unit>
}

data class VoiceProfileState(val work: Boolean, val available: Boolean)

interface VoiceProfileSwitch {
    val profile: Flow<VoiceProfileState?>

    suspend fun setWork(work: Boolean): Outcome<Unit>
}

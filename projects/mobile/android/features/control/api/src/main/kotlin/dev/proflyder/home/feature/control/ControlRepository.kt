package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome

interface ControlRepository {
    val isConfigured: Boolean

    suspend fun catalog(): Outcome<ControlCatalog>

    suspend fun send(entryId: String, action: ControlAction): Outcome<Unit>

    suspend fun adjust(entryId: String, adjust: LightAdjust): Outcome<Unit>

    suspend fun speak(entryId: String, text: String): Outcome<Unit>

    suspend fun setVolume(entryId: String, level: Int): Outcome<Unit>

    suspend fun setVolumeMuted(entryId: String, muted: Boolean): Outcome<Unit>
}

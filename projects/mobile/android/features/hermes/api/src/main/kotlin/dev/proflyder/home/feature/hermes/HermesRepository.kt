package dev.proflyder.home.feature.hermes

import dev.proflyder.home.core.model.HermesAgents
import dev.proflyder.home.core.model.HermesReports
import dev.proflyder.home.core.model.HermesSkills
import dev.proflyder.home.core.model.HermesToolsets
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.VoiceStatus
import kotlinx.coroutines.flow.StateFlow

enum class HermesPanel {
    AGENTS,
    REPORTS,
    VOICE,
    TOOLSETS,
    SKILLS,
}

data class Loaded<T : Any>(
    val loading: Boolean = false,
    val value: T? = null,
    val failure: RemoteFailure? = null,
)

data class HermesState(
    val agents: Loaded<HermesAgents> = Loaded(),
    val reports: Loaded<HermesReports> = Loaded(),
    val voice: Loaded<VoiceStatus> = Loaded(),
    val toolsets: Loaded<HermesToolsets> = Loaded(),
    val skills: Loaded<HermesSkills> = Loaded(),
)

interface HermesRepository {
    val state: StateFlow<HermesState>

    fun refresh(panel: HermesPanel)
}

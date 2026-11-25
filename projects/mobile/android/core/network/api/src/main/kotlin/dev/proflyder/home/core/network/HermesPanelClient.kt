package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.HermesAgents
import dev.proflyder.home.core.model.HermesReports
import dev.proflyder.home.core.model.HermesSkills
import dev.proflyder.home.core.model.HermesToolsets
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.VoiceStatus

interface HermesPanelClient {
    suspend fun agents(): Outcome<HermesAgents>
    suspend fun reports(): Outcome<HermesReports>
    suspend fun voice(): Outcome<VoiceStatus>
    suspend fun skills(): Outcome<HermesSkills>
    suspend fun toolsets(): Outcome<HermesToolsets>
}

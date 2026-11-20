package dev.proflyder.home.feature.system

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SystemSnapshot

interface SystemRepository {
    suspend fun load(): Outcome<SystemSnapshot>
}

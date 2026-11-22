package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.Outcome

interface ControlCatalogClient {
    suspend fun fetch(): Outcome<ControlCatalog>
}

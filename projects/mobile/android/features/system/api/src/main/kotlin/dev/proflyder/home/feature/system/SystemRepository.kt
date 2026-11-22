package dev.proflyder.home.feature.system

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SystemSnapshot
import kotlinx.coroutines.flow.StateFlow

data class SystemState(
    val loading: Boolean = true,
    val snapshot: SystemSnapshot? = null,
    val error: RemoteFailure? = null,
)

interface SystemRepository {
    val state: StateFlow<SystemState>

    fun refresh()
}

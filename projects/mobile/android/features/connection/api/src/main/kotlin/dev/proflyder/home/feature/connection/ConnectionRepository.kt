package dev.proflyder.home.feature.connection

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SchedulerStatus
import kotlinx.coroutines.flow.StateFlow

interface ConnectionRepository {
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>
    val remoteConfig: StateFlow<RemoteConnectionState>

    suspend fun saveToken(update: TokenUpdate): Outcome<ConnectionConfig>

    suspend fun refreshRemoteConfig()

    suspend fun testConnection(): Outcome<SchedulerStatus>
}

sealed interface TokenUpdate {
    data object Keep : TokenUpdate
    data object Clear : TokenUpdate
    data class Replace(val value: String) : TokenUpdate
}

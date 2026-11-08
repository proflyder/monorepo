package dev.proflyder.home.feature.connection

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SchedulerStatus
import kotlinx.coroutines.flow.StateFlow

/** Domain contract for connection settings and a real GetStatus check. */
interface ConnectionRepository {
    /** Effective endpoints/timeouts (Remote Config) plus the device-local token. */
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>
    val remoteConfig: StateFlow<RemoteConnectionState>

    /** Persists only the token; endpoints and timeouts come from Remote Config. */
    suspend fun saveToken(update: TokenUpdate): Outcome<ConnectionConfig>

    /** Fetches and activates the latest Remote Config values. */
    suspend fun refreshRemoteConfig()

    /** Calls home.v1.SchedulerService/GetStatus; never uses mock data. */
    suspend fun testConnection(): Outcome<SchedulerStatus>
}

/** The token is never displayed; blank input means "keep what is stored". */
sealed interface TokenUpdate {
    data object Keep : TokenUpdate
    data object Clear : TokenUpdate
    data class Replace(val value: String) : TokenUpdate
}

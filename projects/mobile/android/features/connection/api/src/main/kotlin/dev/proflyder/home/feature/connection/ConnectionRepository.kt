package dev.proflyder.home.feature.connection

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SchedulerStatus
import kotlinx.coroutines.flow.StateFlow

/** Domain contract for connection settings and a real GetStatus check. */
interface ConnectionRepository {
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>

    suspend fun save(draft: ConnectionDraft): Outcome<ConnectionConfig>

    /** Calls ricardo.v1.SchedulerService/GetStatus; never uses mock data. */
    suspend fun testConnection(): Outcome<SchedulerStatus>
}

data class ConnectionDraft(
    val endpointText: String,
    val tokenUpdate: TokenUpdate,
    val connectTimeoutSeconds: Int,
    val readTimeoutSeconds: Int,
    /** Blank means "no home control service". */
    val controlEndpointText: String = "",
)

/** The token is never displayed; blank input means "keep what is stored". */
sealed interface TokenUpdate {
    data object Keep : TokenUpdate
    data object Clear : TokenUpdate
    data class Replace(val value: String) : TokenUpdate
}

package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map

data class ConnectionConfig(
    val endpoint: ServerEndpoint? = null,
    val controlEndpoint: ServerEndpoint? = null,
    val intercomEndpoint: ServerEndpoint? = null,
    val bearerToken: String? = null,
    val connectTimeoutMillis: Long = DEFAULT_CONNECT_TIMEOUT_MILLIS,
    val readTimeoutMillis: Long = DEFAULT_READ_TIMEOUT_MILLIS,
) {
    val isConfigured: Boolean get() = endpoint != null
    val isIntercomConfigured: Boolean
        get() = intercomEndpoint != null && isTokenValid(bearerToken)
    val hasToken: Boolean get() = isTokenValid(bearerToken)

    fun validatedForStorage(): Outcome<ConnectionConfig> = validatedBy(STORAGE_RULES)

    fun validated(): Outcome<ConnectionConfig> = validatedBy(STORAGE_RULES + REQUEST_RULES)

    private fun validatedBy(rules: List<ValidationRule>): Outcome<ConnectionConfig> =
        rules.firstOrNull { !it.accepts(this) }
            ?.let { Outcome.Failure(RemoteFailure.Validation(it.field)) }
            ?: Outcome.Success(this)

    override fun toString(): String =
        "ConnectionConfig(endpoint=${endpoint?.baseUrl() ?: "<unset>"}, " +
            "controlEndpoint=${controlEndpoint?.baseUrl() ?: "<unset>"}, " +
            "intercomEndpoint=${intercomEndpoint?.baseUrl() ?: "<unset>"}, " +
            "token=${if (hasToken) "<set>" else "<unset>"}, " +
            "connectTimeoutMillis=$connectTimeoutMillis, readTimeoutMillis=$readTimeoutMillis)"

    companion object {
        const val MAX_TOKEN_LENGTH = 4_096

        fun isTokenValid(token: String?): Boolean {
            val value = token?.trim().orEmpty()
            if (value.isEmpty() || value.length > MAX_TOKEN_LENGTH) return false
            return value.all { it.code in 0x21..0x7E }
        }

        const val DEFAULT_CONNECT_TIMEOUT_MILLIS = 5_000L
        const val DEFAULT_READ_TIMEOUT_MILLIS = 20_000L
        const val MIN_TIMEOUT_MILLIS = 1_000L
        const val MAX_TIMEOUT_MILLIS = 120_000L

        private val TIMEOUT_RANGE = MIN_TIMEOUT_MILLIS..MAX_TIMEOUT_MILLIS

        private val STORAGE_RULES = listOf(
            ValidationRule(Field.ENDPOINT) { it.endpoint != null },
            ValidationRule(Field.TIMEOUT) { it.connectTimeoutMillis in TIMEOUT_RANGE },
            ValidationRule(Field.TIMEOUT) { it.readTimeoutMillis in TIMEOUT_RANGE },
            ValidationRule(Field.TOKEN) { it.bearerToken == null || isTokenValid(it.bearerToken) },
        )

        private val REQUEST_RULES = listOf(ValidationRule(Field.TOKEN) { isTokenValid(it.bearerToken) })
    }
}

private class ValidationRule(val field: Field, val accepts: (ConnectionConfig) -> Boolean)

enum class TokenStatus {
    LOADING,
    ABSENT,
    AVAILABLE,
    UNREADABLE,
}

interface ConnectionSettingsStore {
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>

    val loaded: Flow<Boolean> get() = tokenStatus.map { it != TokenStatus.LOADING }

    suspend fun updateToken(bearerToken: String?): Outcome<Unit>
}

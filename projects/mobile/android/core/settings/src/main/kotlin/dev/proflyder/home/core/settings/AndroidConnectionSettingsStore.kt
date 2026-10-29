package dev.proflyder.home.core.settings

import android.content.Context
import android.content.SharedPreferences
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext

/**
 * Persists endpoint/timeouts in ordinary SharedPreferences and the bearer token
 * as Keystore-encrypted ciphertext. The application manifest disables Android
 * backup, so neither file is copied off-device or into cloud backups.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidConnectionSettingsStore(
    context: Context,
    private val cipher: TokenCipher,
) : ConnectionSettingsStore {
    private val appContext = context.applicationContext
    private val prefs: SharedPreferences =
        appContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private val mutableConfig = MutableStateFlow(readInitialConfig())
    override val config: StateFlow<ConnectionConfig> = mutableConfig.asStateFlow()

    private val mutableTokenStatus = MutableStateFlow(
        when {
            !prefs.contains(KEY_TOKEN_CIPHERTEXT) -> TokenStatus.ABSENT
            mutableConfig.value.hasToken -> TokenStatus.AVAILABLE
            else -> TokenStatus.UNREADABLE
        },
    )
    override val tokenStatus: StateFlow<TokenStatus> = mutableTokenStatus.asStateFlow()

    override suspend fun update(config: ConnectionConfig): Outcome<Unit> = withContext(Dispatchers.IO) {
        val validated = config.validatedForStorage()
        if (validated is Outcome.Failure) return@withContext validated

        val endpoint = config.endpoint ?: return@withContext Outcome.Failure(
            RemoteFailure.Validation(dev.proflyder.home.core.model.Field.ENDPOINT),
        )

        val normalizedToken = config.bearerToken?.trim()?.takeIf { it.isNotEmpty() }
        var encrypted: String? = null
        if (normalizedToken != null) {
            encrypted = try {
                cipher.encrypt(normalizedToken)
            } catch (_: Exception) {
                return@withContext Outcome.Failure(
                    RemoteFailure.Storage("The token could not be encrypted on this device."),
                )
            }
        }

        val editor = prefs.edit()
            .putString(KEY_ENDPOINT, endpoint.baseUrl())
            .putString(KEY_CONTROL_ENDPOINT, config.controlEndpoint?.baseUrl())
            .putLong(KEY_CONNECT_TIMEOUT, config.connectTimeoutMillis)
            .putLong(KEY_READ_TIMEOUT, config.readTimeoutMillis)
        if (encrypted != null) {
            editor.putString(KEY_TOKEN_CIPHERTEXT, encrypted)
        } else {
            editor.remove(KEY_TOKEN_CIPHERTEXT)
        }

        if (!editor.commit()) {
            return@withContext Outcome.Failure(
                RemoteFailure.Storage("Settings could not be saved on this device."),
            )
        }

        val stored = ConnectionConfig(
            endpoint = endpoint,
            controlEndpoint = config.controlEndpoint,
            bearerToken = normalizedToken,
            connectTimeoutMillis = config.connectTimeoutMillis,
            readTimeoutMillis = config.readTimeoutMillis,
        )
        mutableConfig.value = stored
        mutableTokenStatus.value = if (normalizedToken != null) TokenStatus.AVAILABLE else TokenStatus.ABSENT
        Outcome.Success(Unit)
    }

    private fun readInitialConfig(): ConnectionConfig {
        val endpoint = prefs.getString(KEY_ENDPOINT, null)
            ?.let { ServerEndpoint.parse(it) as? EndpointParseResult.Valid }
            ?.endpoint

        val controlEndpoint = prefs.getString(KEY_CONTROL_ENDPOINT, null)
            ?.let { ServerEndpoint.parse(it) as? EndpointParseResult.Valid }
            ?.endpoint

        val encrypted = prefs.getString(KEY_TOKEN_CIPHERTEXT, null)
        val token = encrypted?.let(cipher::decrypt)
            ?.takeIf(ConnectionConfig::isTokenValid)

        return ConnectionConfig(
            endpoint = endpoint,
            controlEndpoint = controlEndpoint,
            bearerToken = token,
            connectTimeoutMillis = sanitizedTimeout(
                KEY_CONNECT_TIMEOUT,
                ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS,
            ),
            readTimeoutMillis = sanitizedTimeout(
                KEY_READ_TIMEOUT,
                ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS,
            ),
        )
    }

    private fun sanitizedTimeout(key: String, default: Long): Long {
        val stored = prefs.getLong(key, default)
        return if (stored in ConnectionConfig.MIN_TIMEOUT_MILLIS..ConnectionConfig.MAX_TIMEOUT_MILLIS) {
            stored
        } else {
            default
        }
    }

    companion object {
        private const val PREFS_NAME = "home_connection"
        private const val KEY_ENDPOINT = "endpoint"
        private const val KEY_CONTROL_ENDPOINT = "control_endpoint"
        private const val KEY_TOKEN_CIPHERTEXT = "bearer_token_ciphertext"
        private const val KEY_CONNECT_TIMEOUT = "connect_timeout_millis"
        private const val KEY_READ_TIMEOUT = "read_timeout_millis"
    }
}

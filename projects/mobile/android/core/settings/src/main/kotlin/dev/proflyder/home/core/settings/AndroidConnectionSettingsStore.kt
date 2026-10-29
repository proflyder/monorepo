package dev.proflyder.home.core.settings

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking

/**
 * Persists endpoints/timeouts as plain preferences in the app DataStore and the
 * bearer token only as Keystore-encrypted ciphertext. The manifest disables
 * Android backup, so neither leaves the device.
 *
 * Requests read [config] synchronously (interceptors run on I/O threads), so an
 * in-memory snapshot is the source for readers: it is loaded once when the
 * store is created (one small file read; this is the single place that blocks)
 * and replaced after each successful write.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidConnectionSettingsStore(
    private val dataStore: DataStore<Preferences>,
    private val cipher: TokenCipher,
) : ConnectionSettingsStore {
    private val initial: Preferences = runBlocking {
        dataStore.data.catch { if (it is IOException) emit(emptyPreferences()) else throw it }.first()
    }

    private val mutableConfig = MutableStateFlow(decode(initial))
    override val config: StateFlow<ConnectionConfig> = mutableConfig.asStateFlow()

    private val mutableTokenStatus = MutableStateFlow(
        when {
            !initial.contains(KEY_TOKEN_CIPHERTEXT) -> TokenStatus.ABSENT
            mutableConfig.value.hasToken -> TokenStatus.AVAILABLE
            else -> TokenStatus.UNREADABLE
        },
    )
    override val tokenStatus: StateFlow<TokenStatus> = mutableTokenStatus.asStateFlow()

    override suspend fun update(config: ConnectionConfig): Outcome<Unit> {
        val validated = config.validatedForStorage()
        if (validated is Outcome.Failure) return validated

        val endpoint = config.endpoint ?: return Outcome.Failure(RemoteFailure.Validation(Field.ENDPOINT))

        val normalizedToken = config.bearerToken?.trim()?.takeIf { it.isNotEmpty() }
        val encrypted = if (normalizedToken != null) {
            try {
                cipher.encrypt(normalizedToken)
            } catch (_: Exception) {
                return Outcome.Failure(RemoteFailure.Storage("The token could not be encrypted on this device."))
            }
        } else {
            null
        }

        try {
            dataStore.edit { prefs ->
                prefs[KEY_ENDPOINT] = endpoint.baseUrl()
                val control = config.controlEndpoint?.baseUrl()
                if (control != null) prefs[KEY_CONTROL_ENDPOINT] = control else prefs.remove(KEY_CONTROL_ENDPOINT)
                prefs[KEY_CONNECT_TIMEOUT] = config.connectTimeoutMillis
                prefs[KEY_READ_TIMEOUT] = config.readTimeoutMillis
                if (encrypted != null) prefs[KEY_TOKEN_CIPHERTEXT] = encrypted else prefs.remove(KEY_TOKEN_CIPHERTEXT)
            }
        } catch (_: IOException) {
            return Outcome.Failure(RemoteFailure.Storage("Settings could not be saved on this device."))
        }

        mutableConfig.value = ConnectionConfig(
            endpoint = endpoint,
            controlEndpoint = config.controlEndpoint,
            bearerToken = normalizedToken,
            connectTimeoutMillis = config.connectTimeoutMillis,
            readTimeoutMillis = config.readTimeoutMillis,
        )
        mutableTokenStatus.value = if (normalizedToken != null) TokenStatus.AVAILABLE else TokenStatus.ABSENT
        return Outcome.Success(Unit)
    }

    private fun decode(prefs: Preferences): ConnectionConfig {
        fun endpointOf(key: Preferences.Key<String>): ServerEndpoint? =
            prefs[key]?.let { ServerEndpoint.parse(it) as? EndpointParseResult.Valid }?.endpoint

        val token = prefs[KEY_TOKEN_CIPHERTEXT]
            ?.let(cipher::decrypt)
            ?.takeIf(ConnectionConfig::isTokenValid)

        return ConnectionConfig(
            endpoint = endpointOf(KEY_ENDPOINT),
            controlEndpoint = endpointOf(KEY_CONTROL_ENDPOINT),
            bearerToken = token,
            connectTimeoutMillis = sanitizedTimeout(
                prefs[KEY_CONNECT_TIMEOUT],
                ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS,
            ),
            readTimeoutMillis = sanitizedTimeout(
                prefs[KEY_READ_TIMEOUT],
                ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS,
            ),
        )
    }

    private fun sanitizedTimeout(stored: Long?, default: Long): Long =
        if (stored != null && stored in ConnectionConfig.MIN_TIMEOUT_MILLIS..ConnectionConfig.MAX_TIMEOUT_MILLIS) {
            stored
        } else {
            default
        }

    internal companion object {
        // Same names as the legacy SharedPreferences keys, so the migration maps 1:1.
        val KEY_ENDPOINT = stringPreferencesKey("endpoint")
        val KEY_CONTROL_ENDPOINT = stringPreferencesKey("control_endpoint")
        val KEY_TOKEN_CIPHERTEXT = stringPreferencesKey("bearer_token_ciphertext")
        val KEY_CONNECT_TIMEOUT = longPreferencesKey("connect_timeout_millis")
        val KEY_READ_TIMEOUT = longPreferencesKey("read_timeout_millis")
    }
}

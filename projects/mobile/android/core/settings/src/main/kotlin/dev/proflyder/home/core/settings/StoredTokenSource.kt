package dev.proflyder.home.core.settings

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.StorageReason
import dev.proflyder.home.core.operation.suspendRunCatching
import java.io.IOException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.withContext

internal data class StoredToken(val status: TokenStatus, val token: String? = null) {
    companion object {
        val Loading = StoredToken(TokenStatus.LOADING)
    }
}

internal class StoredTokenSource(
    private val dataStore: DataStore<Preferences>,
    private val cipher: TokenCipher,
    private val io: CoroutineDispatcher,
    scope: CoroutineScope,
) {
    val state: StateFlow<StoredToken> = dataStore.data
        .catch { if (it is IOException) emit(emptyPreferences()) else throw it }
        .map(::decode)
        .flowOn(io)
        .stateIn(scope, SharingStarted.Eagerly, StoredToken.Loading)

    suspend fun write(token: String?): Outcome<Unit> {
        val encrypted = withContext(io) { runCatching { token?.let(cipher::encrypt) } }
            .getOrElse { return Outcome.Failure(RemoteFailure.Storage(StorageReason.TOKEN_ENCRYPTION)) }
        suspendRunCatching { dataStore.edit { it.store(encrypted) } }.onFailure { failure ->
            if (failure !is IOException) throw failure
            return Outcome.Failure(RemoteFailure.Storage(StorageReason.SETTINGS_WRITE))
        }
        state.first { it.status != TokenStatus.LOADING && it.token == token }
        return Outcome.Success(Unit)
    }

    private fun decode(preferences: Preferences): StoredToken {
        val ciphertext = preferences[KEY_TOKEN_CIPHERTEXT] ?: return StoredToken(TokenStatus.ABSENT)
        val token = cipher.decrypt(ciphertext)?.takeIf(ConnectionConfig::isTokenValid)
        return StoredToken(if (token != null) TokenStatus.AVAILABLE else TokenStatus.UNREADABLE, token)
    }

    private fun androidx.datastore.preferences.core.MutablePreferences.store(ciphertext: String?) {
        if (ciphertext != null) this[KEY_TOKEN_CIPHERTEXT] = ciphertext else remove(KEY_TOKEN_CIPHERTEXT)
    }

    private companion object {
        val KEY_TOKEN_CIPHERTEXT = stringPreferencesKey("bearer_token_ciphertext")
    }
}

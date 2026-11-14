package dev.proflyder.home.feature.push

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update

/**
 * Last accepted registration in the app-wide DataStore.
 *
 * The token, its timestamp and the owner-visible name are stored: the token is
 * not a credential
 * for the home server (the bearer token is), but it is still never logged or
 * shown in full, and `allowBackup` is off, so it never leaves the device.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStorePushRegistrationStore(
    private val dataStore: DataStore<Preferences>,
) : PushRegistrationStore {
    private val state = MutableStateFlow(PushRegistration())

    override val registration: StateFlow<PushRegistration> = state.asStateFlow()

    override suspend fun load() {
        suspendRunCatching {
            val preferences = dataStore.data.first()
            state.update {
                it.copy(
                    token = preferences[KEY_TOKEN],
                    registeredAtMillis = preferences[KEY_REGISTERED_AT] ?: 0,
                    deviceName = preferences[KEY_DEVICE_NAME].orEmpty(),
                )
            }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    override suspend fun saved(token: String, registeredAtMillis: Long) {
        suspendRunCatching {
            dataStore.edit {
                it[KEY_TOKEN] = token
                it[KEY_REGISTERED_AT] = registeredAtMillis
            }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
        state.update { it.copy(token = token, registeredAtMillis = registeredAtMillis) }
    }

    override suspend fun savedName(name: String) {
        suspendRunCatching {
            dataStore.edit { it[KEY_DEVICE_NAME] = name }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
        state.update { it.copy(deviceName = name) }
    }

    override suspend fun busy(busy: Boolean) {
        state.update { it.copy(busy = busy) }
    }

    override suspend fun failed(failed: Boolean) {
        state.update { it.copy(failed = failed) }
    }

    private companion object {
        val KEY_TOKEN = stringPreferencesKey("push_token")
        val KEY_REGISTERED_AT = longPreferencesKey("push_registered_at")
        val KEY_DEVICE_NAME = stringPreferencesKey("push_device_name")
    }
}

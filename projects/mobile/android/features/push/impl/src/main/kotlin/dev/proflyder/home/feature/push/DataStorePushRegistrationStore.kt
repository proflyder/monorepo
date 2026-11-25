package dev.proflyder.home.feature.push

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.MutablePreferences
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
                    appVersion = preferences[KEY_APP_VERSION].orEmpty(),
                    server = preferences[KEY_SERVER].orEmpty(),
                )
            }
            dataStore.edit(::forgetManualName)
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    override suspend fun saved(record: PushRegistrationRecord) {
        suspendRunCatching {
            dataStore.edit { it.putRegistration(record) }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
        state.update {
            it.copy(
                token = record.token,
                registeredAtMillis = record.registeredAtMillis,
                appVersion = record.appVersion,
                server = record.server,
            )
        }
    }

    private fun forgetManualName(preferences: MutablePreferences) {
        preferences.remove(KEY_LEGACY_DEVICE_NAME)
        preferences.remove(KEY_LEGACY_REGISTERED_NAME)
    }

    private fun MutablePreferences.putRegistration(record: PushRegistrationRecord) {
        this[KEY_TOKEN] = record.token
        this[KEY_REGISTERED_AT] = record.registeredAtMillis
        this[KEY_APP_VERSION] = record.appVersion
        this[KEY_SERVER] = record.server
    }

    private companion object {
        val KEY_TOKEN = stringPreferencesKey("push_token")
        val KEY_REGISTERED_AT = longPreferencesKey("push_registered_at")
        val KEY_LEGACY_DEVICE_NAME = stringPreferencesKey("push_device_name")
        val KEY_LEGACY_REGISTERED_NAME = stringPreferencesKey("push_registered_name")
        val KEY_APP_VERSION = stringPreferencesKey("push_registered_app_version")
        val KEY_SERVER = stringPreferencesKey("push_registered_server")
    }
}

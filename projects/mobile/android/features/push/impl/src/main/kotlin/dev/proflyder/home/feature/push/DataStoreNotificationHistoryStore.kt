package dev.proflyder.home.feature.push

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
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

/**
 * The received-message log in the app-wide DataStore, one JSON value under its
 * own key (the notification text is not a credential; `allowBackup` is off, so
 * it never leaves the device).
 *
 * Every write reads the persisted value inside the same `edit` transaction, so
 * the FCM service can record a message before any screen loaded the log without
 * overwriting earlier entries.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreNotificationHistoryStore(
    private val dataStore: DataStore<Preferences>,
) : NotificationHistoryStore {
    private val state = MutableStateFlow<List<NotificationHistoryEntry>>(emptyList())

    override val entries: StateFlow<List<NotificationHistoryEntry>> = state.asStateFlow()

    override suspend fun load() {
        suspendRunCatching {
            NotificationHistoryCodec.decode(dataStore.data.first()[KEY_ENTRIES])
        }.onSuccess { stored ->
            state.value = stored
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    override suspend fun record(entry: NotificationHistoryEntry) {
        suspendRunCatching {
            dataStore.edit { preferences ->
                val updated = NotificationHistoryRules.record(
                    entries = NotificationHistoryCodec.decode(preferences[KEY_ENTRIES]),
                    entry = entry,
                )
                preferences[KEY_ENTRIES] = NotificationHistoryCodec.encode(updated)
                state.value = updated
            }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    override suspend fun clear() {
        suspendRunCatching {
            dataStore.edit { preferences -> preferences.remove(KEY_ENTRIES) }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
        state.value = emptyList()
    }

    private companion object {
        val KEY_ENTRIES = stringPreferencesKey("push_notification_history")
    }
}

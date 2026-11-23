package dev.proflyder.home.feature.push

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@SingleIn(PushHistoryScope::class)
@Inject
internal class DataStoreNotificationHistoryStore(
    private val dataStore: DataStore<Preferences>,
    lifetime: FeatureLifetime,
) : NotificationHistoryStore {
    override val entries: StateFlow<List<NotificationHistoryEntry>> = dataStore.data
        .map { preferences -> NotificationHistoryCodec.decode(preferences[KEY_ENTRIES]) }
        .catch { failure -> if (failure is IOException) emit(emptyList()) else throw failure }
        .stateIn(lifetime.scope, SharingStarted.Eagerly, emptyList())

    override suspend fun record(entry: NotificationHistoryEntry) = write { preferences ->
        val updated = NotificationHistoryRules.record(NotificationHistoryCodec.decode(preferences[KEY_ENTRIES]), entry)
        preferences[KEY_ENTRIES] = NotificationHistoryCodec.encode(updated)
    }

    override suspend fun clear() = write { preferences -> preferences.remove(KEY_ENTRIES) }

    private suspend fun write(change: (androidx.datastore.preferences.core.MutablePreferences) -> Unit) {
        suspendRunCatching { dataStore.edit(change) }
            .onFailure { failure -> if (failure !is IOException) throw failure }
    }

    private companion object {
        val KEY_ENTRIES = stringPreferencesKey("push_notification_history")
    }
}

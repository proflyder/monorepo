package dev.proflyder.home.feature.update.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext

internal interface UpdateCheckStore {
    suspend fun lastCheckMillis(): Long

    suspend fun recordCheck(millis: Long)
}

@SingleIn(AppScope::class)
@Inject
internal class DataStoreUpdateCheckStore(
    private val dataStore: DataStore<Preferences>,
    @IoDispatcher private val io: CoroutineDispatcher,
) : UpdateCheckStore {
    override suspend fun lastCheckMillis(): Long = withContext(io) {
        suspendRunCatching { dataStore.data.first()[KEY] }.getOrNull() ?: 0L
    }

    override suspend fun recordCheck(millis: Long) {
        withContext(io) {
            suspendRunCatching { dataStore.edit { it[KEY] = millis } }
        }
    }

    private companion object {
        val KEY = longPreferencesKey("update_last_check_wall_millis")
    }
}

package dev.proflyder.home.feature.control.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlPreferences
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreControlPreferences(
    private val dataStore: DataStore<Preferences>,
) : ControlPreferences {
    override val filter: Flow<ControlFilter?> = dataStore.data
        .catch { failure -> if (failure is IOException) emit(emptyPreferences()) else throw failure }
        .map { ControlFilter.decode(it[KEY_FILTER]) }
        .distinctUntilChanged()

    override suspend fun saveFilter(filter: ControlFilter) {
        suspendRunCatching {
            dataStore.edit { it[KEY_FILTER] = ControlFilter.encode(filter) }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
        }
    }

    private companion object {
        val KEY_FILTER = stringPreferencesKey("control_filter")
    }
}

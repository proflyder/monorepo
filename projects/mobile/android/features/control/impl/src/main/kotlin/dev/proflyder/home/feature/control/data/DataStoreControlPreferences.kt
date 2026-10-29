package dev.proflyder.home.feature.control.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlPreferences
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.first

/** Keeps the Home screen's choices in the app-wide DataStore under its own keys. */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreControlPreferences(
    private val dataStore: DataStore<Preferences>,
) : ControlPreferences {
    override suspend fun loadFilter(): ControlFilter? = try {
        ControlFilter.decode(dataStore.data.first()[KEY_FILTER])
    } catch (_: IOException) {
        null
    }

    override suspend fun saveFilter(filter: ControlFilter) {
        try {
            dataStore.edit { it[KEY_FILTER] = ControlFilter.encode(filter) }
        } catch (_: IOException) {
            // The choice just is not remembered; the screen keeps working.
        }
    }

    private companion object {
        val KEY_FILTER = stringPreferencesKey("control_filter")
    }
}

package dev.proflyder.home.core.settings

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.core.handlers.ReplaceFileCorruptionHandler
import androidx.datastore.preferences.SharedPreferencesMigration
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.preferencesDataStoreFile
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * The app's single Jetpack DataStore (Preferences): server connection settings
 * and UI preferences live in one file, each owner using its own keys.
 *
 * The first run after upgrading migrates the previous SharedPreferences file
 * ([LEGACY_CONNECTION_PREFS]) so saved endpoints and the encrypted token survive.
 * A corrupted file is replaced by empty preferences instead of crashing.
 *
 * Only one DataStore may be active per file: create exactly one per process
 * (the app graph does); tests pass a unique [fileName].
 */
object HomeDataStore {
    const val DEFAULT_FILE_NAME = "home_prefs"
    const val LEGACY_CONNECTION_PREFS = "home_connection"

    fun create(
        context: Context,
        fileName: String = DEFAULT_FILE_NAME,
        scope: CoroutineScope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
    ): DataStore<Preferences> {
        val appContext = context.applicationContext
        return PreferenceDataStoreFactory.create(
            corruptionHandler = ReplaceFileCorruptionHandler { emptyPreferences() },
            migrations = listOf(SharedPreferencesMigration(appContext, LEGACY_CONNECTION_PREFS)),
            scope = scope,
            produceFile = { appContext.preferencesDataStoreFile(fileName) },
        )
    }
}

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

package dev.proflyder.home.core.settings

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Favorites
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.first

/** Pinned Home shortcuts in the app-wide DataStore, ordered as the user pinned them. */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreFavoritePreferences(
    private val dataStore: DataStore<Preferences>,
) : FavoritePreferences {
    override suspend fun load(): List<FavoriteRef>? = suspendRunCatching {
        dataStore.data.first()[KEY_FAVORITES]?.let { raw ->
            Favorites.decode(raw.split(SEPARATOR).filter { it.isNotBlank() })
        }
    }.getOrElse { null }

    override suspend fun save(refs: List<FavoriteRef>) {
        suspendRunCatching {
            dataStore.edit {
                it[KEY_FAVORITES] = Favorites.encode(refs).joinToString(SEPARATOR)
            }
        }.onFailure { failure ->
            if (failure !is IOException) throw failure
            // The choice just is not remembered; the screen keeps working.
        }
    }

    private companion object {
        val KEY_FAVORITES = stringPreferencesKey("home_favorites")
        const val SEPARATOR = "\n"
    }
}

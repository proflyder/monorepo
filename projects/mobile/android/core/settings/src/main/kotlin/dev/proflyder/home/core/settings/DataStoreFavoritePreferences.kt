package dev.proflyder.home.core.settings

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
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
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreFavoritePreferences(
    private val dataStore: DataStore<Preferences>,
) : FavoritePreferences {
    override val favorites: Flow<List<FavoriteRef>?> = dataStore.data
        .catch { failure -> if (failure is IOException) emit(emptyPreferences()) else throw failure }
        .map { it.storedRefs() }
        .distinctUntilChanged()

    override suspend fun saveIfAbsent(refs: List<FavoriteRef>) {
        write { if (it.storedRefs() == null) it.store(refs) }
    }

    override suspend fun toggle(ref: FavoriteRef): Boolean {
        val saved = write { it.store(Favorites.toggle(it.storedRefs().orEmpty(), ref)) }
        return ref in saved?.storedRefs().orEmpty()
    }

    private suspend fun write(change: (MutablePreferences) -> Unit): Preferences? =
        suspendRunCatching { dataStore.edit(change) }
            .onFailure { failure -> if (failure !is IOException) throw failure }
            .getOrNull()

    private fun Preferences.storedRefs(): List<FavoriteRef>? =
        this[KEY_FAVORITES]?.let { raw -> Favorites.decode(raw.split(SEPARATOR).filter { it.isNotBlank() }) }

    private fun MutablePreferences.store(refs: List<FavoriteRef>) {
        this[KEY_FAVORITES] = Favorites.encode(refs).joinToString(SEPARATOR)
    }

    private companion object {
        val KEY_FAVORITES = stringPreferencesKey("home_favorites")
        const val SEPARATOR = "\n"
    }
}

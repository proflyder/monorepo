package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Favorites
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.feature.control.ControlCatalogState
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach

internal class ControlFavorites(private val preferences: FavoritePreferences) {
    fun resolved(catalog: Flow<ControlCatalogState>, scenarios: Flow<List<Scenario>?>): Flow<List<FavoriteRef>> =
        combine(preferences.favorites, catalog, scenarios, ::resolve)
            .onEach { resolution -> resolution.seed?.let { preferences.saveIfAbsent(it) } }
            .map { it.refs }

    suspend fun toggle(subject: PinSubject): ControlEvent =
        ControlEvent.Notice(ControlNotice.FavoriteChanged(subject, preferences.toggle(subject.ref)))

    private fun resolve(
        stored: List<FavoriteRef>?,
        catalog: ControlCatalogState,
        scenarios: List<Scenario>?,
    ): Resolution {
        if (stored != null) return Resolution(stored, seed = null)
        val loaded = catalog.catalog ?: return Resolution(emptyList(), seed = null)
        val known = scenarios ?: return Resolution(emptyList(), seed = null)
        val defaults = Favorites.defaults(loaded.entries, known)
        return Resolution(defaults, seed = defaults)
    }

    private data class Resolution(val refs: List<FavoriteRef>, val seed: List<FavoriteRef>?)
}

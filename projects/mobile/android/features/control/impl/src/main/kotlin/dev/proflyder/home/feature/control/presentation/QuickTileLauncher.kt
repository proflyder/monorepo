package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTileAction
import dev.proflyder.home.feature.control.QuickTileProvider

internal class QuickTileLauncher(private val providers: Set<QuickTileProvider>) {
    val tiles: List<ProvidedTile> get() = providers.map { it.tile }

    fun activate(key: String): ControlEvent? =
        when (val action = providers.firstOrNull { it.tile.key == key }?.activate()) {
            is QuickTileAction.OpenFeature -> ControlEvent.OpenRoute(Route.Tab(action.featureId))
            QuickTileAction.Handled, null -> null
        }
}

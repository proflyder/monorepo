package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTileAction
import dev.proflyder.home.feature.control.QuickTileProvider

internal class QuickTileLauncher(
    private val providers: Set<QuickTileProvider>,
    private val access: AppAccessSource,
) {
    val tiles: List<ProvidedTile>
        get() = providers.map { it.tile }.filter { access.state.value.allows(it.module) }

    fun activate(key: String): ControlEvent? =
        when (val action = visibleProvider(key)?.activate()) {
            is QuickTileAction.OpenFeature -> ControlEvent.OpenRoute(Route.Tab(action.featureId))
            QuickTileAction.Handled, null -> null
        }

    private fun visibleProvider(key: String): QuickTileProvider? =
        providers.firstOrNull { it.tile.key == key && access.state.value.allows(it.tile.module) }
}

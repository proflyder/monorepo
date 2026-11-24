package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.AppModule

data class ProvidedTile(
    val key: String,
    val order: Int,
    val labelRes: Int,
    val iconRes: Int,
    val module: AppModule = AppModule.BASE,
)

sealed interface QuickTileAction {
    data object Handled : QuickTileAction

    data class OpenFeature(val featureId: String) : QuickTileAction
}

interface QuickTileProvider {
    val tile: ProvidedTile

    fun activate(): QuickTileAction
}

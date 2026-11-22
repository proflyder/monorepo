package dev.proflyder.home.feature.control

data class ProvidedTile(
    val key: String,
    val order: Int,
    val labelRes: Int,
    val iconRes: Int,
)

sealed interface QuickTileAction {
    data object Handled : QuickTileAction

    data class OpenFeature(val featureId: String) : QuickTileAction
}

interface QuickTileProvider {
    val tile: ProvidedTile

    fun activate(): QuickTileAction
}

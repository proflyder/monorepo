package dev.proflyder.home.feature.control

internal sealed interface SpeedDialItem {
    val key: String

    data class Tile(val tile: QuickTile) : SpeedDialItem {
        override val key: String get() = "tile:${tile.key}"
    }

    data class Wake(val action: WakeFeedbackAction) : SpeedDialItem {
        override val key: String get() = "wake:${action.kind.name}"
    }
}

internal object SpeedDialItems {
    fun build(
        tiles: List<QuickTile>,
        wake: List<WakeFeedbackAction>,
    ): List<SpeedDialItem> = tiles.map(SpeedDialItem::Tile) + wake.map(SpeedDialItem::Wake)
}

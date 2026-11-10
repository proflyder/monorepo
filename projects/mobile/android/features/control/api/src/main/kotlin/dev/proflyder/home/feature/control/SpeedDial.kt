package dev.proflyder.home.feature.control

/**
 * One row of the Home quick-actions dial. The dial is built from the existing
 * quick tiles (call, speak, pinned devices and scenarios) plus the resolved
 * wake-feedback marks, so a newly pinned favourite appears automatically.
 */
sealed interface SpeedDialItem {
    val key: String

    data class Tile(val tile: QuickTile) : SpeedDialItem {
        override val key: String get() = "tile:${tile.key}"
    }

    data class Wake(val action: WakeFeedbackAction) : SpeedDialItem {
        override val key: String get() = "wake:${action.kind.name}"
    }
}

/** Builds the dial in order: quick tiles first, then the wake marks. */
object SpeedDialItems {
    fun build(
        tiles: List<QuickTile>,
        wake: List<WakeFeedbackAction>,
    ): List<SpeedDialItem> = tiles.map(SpeedDialItem::Tile) + wake.map(SpeedDialItem::Wake)
}

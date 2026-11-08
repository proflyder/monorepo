package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Scenario

/** One pinned tile in "Quick actions". */
sealed interface QuickTile {
    val key: String

    data class Device(val entry: ControlEntry, val busy: Boolean) : QuickTile {
        override val key: String get() = "device:${entry.id}"
    }

    data class ScenarioTile(val scenario: Scenario, val busy: Boolean) : QuickTile {
        override val key: String get() = "scenario:${scenario.id}"
    }

    /** "Say on the station": a catalog action entry, opened as a text dialog. */
    data class Speak(val entry: ControlEntry, val busy: Boolean) : QuickTile {
        override val key: String get() = "action:${entry.id}"
    }
}

/**
 * Resolves ordered [FavoriteRef]s against the loaded catalog and scenario list.
 * Pins whose target disappeared are skipped instead of rendered as empty tiles.
 */
object QuickTiles {
    fun build(
        refs: List<FavoriteRef>,
        entries: List<ControlEntry>,
        scenarios: List<Scenario>,
        pendingEntries: Set<String> = emptySet(),
        pendingScenarios: Set<String> = emptySet(),
    ): List<QuickTile> {
        val entriesById = entries.associateBy { it.id }
        val scenariosById = scenarios.associateBy { it.id }
        return refs.mapNotNull { ref ->
            when (ref.kind) {
                FavoriteKind.DEVICE -> entriesById[ref.id]
                    ?.takeIf { it.isDevice }
                    ?.let { QuickTile.Device(it, it.id in pendingEntries) }
                FavoriteKind.ACTION -> entriesById[ref.id]
                    ?.takeIf { it.kind == ControlKind.ACTION }
                    ?.let { QuickTile.Speak(it, it.id in pendingEntries) }
                FavoriteKind.SCENARIO -> scenariosById[ref.id]
                    ?.let { QuickTile.ScenarioTile(it, it.id in pendingScenarios) }
            }
        }
    }
}

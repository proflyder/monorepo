package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Scenario

internal sealed interface QuickTile {
    val key: String

    data class Device(val entry: ControlEntry, val busy: Boolean) : QuickTile {
        override val key: String get() = "device:${entry.id}"
    }

    data class ScenarioTile(val scenario: Scenario, val busy: Boolean) : QuickTile {
        override val key: String get() = "scenario:${scenario.id}"
    }

    data class Speak(val entry: ControlEntry, val busy: Boolean) : QuickTile {
        override val key: String get() = "action:${entry.id}"
    }

    data class Provided(val tile: ProvidedTile) : QuickTile {
        override val key: String get() = "provided:${tile.key}"
    }
}

internal object QuickTiles {
    fun build(
        provided: List<ProvidedTile>,
        refs: List<FavoriteRef>,
        entries: List<ControlEntry>,
        scenarios: List<Scenario>,
        pendingEntries: Set<String> = emptySet(),
        pendingScenarios: Set<String> = emptySet(),
    ): List<QuickTile> {
        val entriesById = entries.associateBy { it.id }
        val scenariosById = scenarios.associateBy { it.id }
        val fixed = provided.sortedBy { it.order }.map(QuickTile::Provided)
        return fixed + refs.mapNotNull { ref ->
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

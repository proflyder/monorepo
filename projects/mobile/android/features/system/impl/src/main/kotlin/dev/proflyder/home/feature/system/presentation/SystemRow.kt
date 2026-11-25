package dev.proflyder.home.feature.system.presentation

import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.core.navigation.Route

internal sealed interface SystemRow {
    val key: String

    data class Status(val level: HealthLevel, val title: String, val subtitle: String) : SystemRow {
        override val key: String = "status"
    }

    data class Card(
        val section: SystemSection,
        val level: HealthLevel,
        val title: String,
        val lines: String,
    ) : SystemRow {
        override val key: String = "card:${section.route}"
    }

    data class Group(override val key: String, val level: HealthLevel, val title: String) : SystemRow

    data class Item(
        override val key: String,
        val level: HealthLevel,
        val title: String,
        val subtitle: String,
    ) : SystemRow

    data class Note(override val key: String, val text: String) : SystemRow

    data class Link(
        override val key: String,
        val title: String,
        val subtitle: String,
        val route: Route,
    ) : SystemRow
}

internal data class SystemLinks(
    val access: Boolean = false,
    val secrets: Boolean = false,
    val hermes: Boolean = false,
)

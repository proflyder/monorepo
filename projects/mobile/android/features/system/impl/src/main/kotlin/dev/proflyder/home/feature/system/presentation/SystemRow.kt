package dev.proflyder.home.feature.system.presentation

import dev.proflyder.home.core.model.HealthLevel

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
}

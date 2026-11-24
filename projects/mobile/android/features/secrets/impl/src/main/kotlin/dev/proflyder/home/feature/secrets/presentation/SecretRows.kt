package dev.proflyder.home.feature.secrets.presentation

import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.model.SecretVersion

internal sealed interface SecretsCatalogRow {
    val key: String

    data object DeviceCard : SecretsCatalogRow {
        override val key: String = "device-card"
    }

    data class Group(override val key: String, val name: String) : SecretsCatalogRow

    data class EntryRow(override val key: String, val entry: SecretEntry) : SecretsCatalogRow
}

internal fun catalogRows(
    groups: List<SecretsCatalogGroup>,
    deviceCard: Boolean = false,
): List<SecretsCatalogRow> =
    (if (deviceCard) listOf(SecretsCatalogRow.DeviceCard) else emptyList()) +
        groups.flatMap { group ->
            listOf<SecretsCatalogRow>(SecretsCatalogRow.Group("group:${group.name}", group.name)) +
                group.entries.map { SecretsCatalogRow.EntryRow("path:${it.path}", it) }
        }

internal sealed interface SecretPathRow {
    val key: String

    data class Header(
        override val key: String,
        val title: String,
        val description: String,
        val currentVersion: Long,
        val sessionActive: Boolean,
    ) : SecretPathRow

    data class KeyRow(
        override val key: String,
        val name: String,
        val value: String?,
        val revealed: Boolean,
        val version: Long,
        val revealing: Boolean,
    ) : SecretPathRow

    data class VersionRow(
        override val key: String,
        val version: SecretVersion,
        val current: Boolean,
        val rollbackLabel: String,
        val createdLabel: String,
        val unavailableLabel: String?,
    ) : SecretPathRow

    data class Note(override val key: String, val text: String) : SecretPathRow
}

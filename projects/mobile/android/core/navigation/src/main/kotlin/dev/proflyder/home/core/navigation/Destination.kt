package dev.proflyder.home.core.navigation

import android.os.Bundle
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.fragment.app.Fragment

interface Destination {
    val featureId: String
    val route: String

    fun arguments(): Bundle = Bundle.EMPTY
}

interface FeatureEntry {
    val id: String
    val order: Int

    val isTab: Boolean get() = true

    @get:StringRes
    val titleRes: Int

    @get:DrawableRes
    val iconRes: Int

    val rootDestination: Destination

    @StringRes
    fun titleRes(destination: Destination): Int = titleRes

    fun ownsRoute(route: String): Boolean =
        route == rootDestination.route || route.startsWith("$id/")

    fun supports(destination: Destination): Boolean

    fun createFragment(destination: Destination): Fragment

    fun hostedDestination(featureId: String): Destination? = null
}

class FeatureRegistry(entries: Collection<FeatureEntry>) {
    private val byId: Map<String, FeatureEntry>
    private val ordered: List<FeatureEntry>

    init {
        require(entries.isNotEmpty()) { "at least one feature entry is required" }
        val duplicates = entries.groupBy { it.id }.filterValues { it.size > 1 }.keys
        require(duplicates.isEmpty()) { "duplicate feature ids: $duplicates" }
        byId = entries.associateBy { it.id }
        ordered = entries.sortedWith(compareBy({ it.order }, { it.id }))
    }

    fun all(): List<FeatureEntry> = ordered

    fun tabs(): List<FeatureEntry> = ordered.filter { it.isTab }

    fun entryFor(destination: Destination): FeatureEntry? = byId[destination.featureId]

    fun entry(id: String): FeatureEntry? = byId[id]

    fun entryForRoute(route: String): FeatureEntry? {
        if (route.isBlank()) return null
        return ordered.firstOrNull { it.ownsRoute(route) }
    }
}

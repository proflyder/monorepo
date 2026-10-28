package dev.proflyder.home.core.navigation

import android.os.Bundle
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.fragment.app.Fragment

/**
 * A typed navigation target. Feature modules define concrete data classes (for
 * example `AlarmEditorDestination(id)`) and the router resolves them through
 * [FeatureEntry]. Only stable identifiers belong in [arguments]; secrets are
 * never placed in a destination or saved state.
 */
interface Destination {
    val featureId: String
    val route: String

    fun arguments(): Bundle = Bundle.EMPTY
}

/**
 * Registration point for one bottom-tab feature. Implementations live in the
 * feature implementation module and are contributed into the app graph.
 */
interface FeatureEntry {
    val id: String
    val order: Int

    @get:StringRes
    val titleRes: Int

    @get:DrawableRes
    val iconRes: Int

    val rootDestination: Destination

    /** Toolbar title for a destination; editors may override the tab title. */
    @StringRes
    fun titleRes(destination: Destination): Int = titleRes

    /**
     * Whether [route] belongs to this feature. Routes are namespaced by feature
     * id (`alarms/list`, `alarms/editor/new`), so saved route strings can be
     * validated after process recreation without reconstructing a Destination.
     */
    fun ownsRoute(route: String): Boolean =
        route == rootDestination.route || route.startsWith("$id/")

    fun supports(destination: Destination): Boolean

    /** Fragments are no-arg; destination data is supplied via arguments. */
    fun createFragment(destination: Destination): Fragment
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

    fun entryFor(destination: Destination): FeatureEntry? = byId[destination.featureId]

    fun entry(id: String): FeatureEntry? = byId[id]

    /** Resolves a persisted route string to its registered feature, if any. */
    fun entryForRoute(route: String): FeatureEntry? {
        if (route.isBlank()) return null
        return ordered.firstOrNull { it.ownsRoute(route) }
    }
}

package dev.proflyder.home.core.navigation

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.fragment.app.Fragment

interface Destination {
    val featureId: String
    val route: String
}

interface FeatureEntry {
    val id: String
    val order: Int

    val isTab: Boolean get() = true

    val hidesBottomBar: Boolean get() = false

    @get:StringRes
    val titleRes: Int

    @get:DrawableRes
    val iconRes: Int

    val rootDestination: Destination

    val routes: List<RouteCodec<*>> get() = listOf(RouteCodec.fixed(rootDestination))

    @StringRes
    fun titleRes(destination: Destination): Int = titleRes

    fun parse(path: RoutePath): Destination? = routes.firstNotNullOfOrNull { it.decode(path) }

    fun deepLink(path: RoutePath): Destination? = parse(path)

    fun supports(destination: Destination): Boolean =
        RoutePath.of(destination.route)?.let(::parse) == destination

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
        val path = RoutePath.of(route) ?: return null
        return ordered.firstOrNull { it.parse(path) != null }
    }
}

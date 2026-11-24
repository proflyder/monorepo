package dev.proflyder.home.core.navigation

import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.access.owner
import dev.proflyder.home.core.model.AppModule

data class NotificationTarget(
    val route: Route,
    val module: AppModule,
    val ownerOnly: Boolean = false,
)

fun NotificationTarget.routeWhenAllowed(access: AppAccessState): Route? =
    route.takeIf { access.allows(module) && (!ownerOnly || access.owner) }

class NotificationTargets(private val features: FeatureRegistry) {
    fun of(link: String?): NotificationTarget? {
        val path = pathOf(link) ?: return null
        return featureById(path) ?: featureByRoute(path)
    }

    private fun pathOf(link: String?): RoutePath? {
        val value = link?.trim()?.removePrefix(LINK_SCHEME).orEmpty()
        return RoutePath.of(value)
    }

    private fun featureById(path: RoutePath): NotificationTarget? {
        val entry = features.entry(path.value)?.takeIf(FeatureEntry::notificationLink) ?: return null
        val hosted = entry.isTab || features.tabs().any { it.hostedDestination(entry.id) != null }
        val route = if (hosted) Route.Tab(entry.id) else Route.Link(entry.rootDestination.route)
        return NotificationTarget(route, entry.module, entry.ownerOnly)
    }

    private fun featureByRoute(path: RoutePath): NotificationTarget? {
        val entry = features.entryForRoute(path.value)?.takeIf(FeatureEntry::notificationLink) ?: return null
        val destination = entry.parse(path) ?: return null
        val route = hostedSection(entry, destination)?.let { section -> Route.Tab(section.id) }
            ?: rootTab(entry, destination, path)
            ?: Route.Link(path.value)
        return NotificationTarget(route, entry.module, entry.ownerOnly)
    }

    private fun hostedSection(entry: FeatureEntry, destination: Destination): FeatureEntry? =
        features.all().firstOrNull { candidate ->
            !candidate.isTab && entry.hostedDestination(candidate.id) == destination
        }

    private fun rootTab(entry: FeatureEntry, destination: Destination, path: RoutePath): Route? =
        Route.Tab(entry.id).takeIf {
            entry.isTab && destination == entry.rootDestination && path.value == entry.id
        }

    private companion object {
        const val LINK_SCHEME = "home://"
    }
}

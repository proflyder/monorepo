package dev.proflyder.home.core.navigation

sealed interface Route {
    data class Screen(val destination: Destination) : Route

    data class Tab(val featureId: String) : Route

    data class Link(val path: String) : Route

    data object Back : Route
}

fun Navigator.open(route: Route) {
    when (route) {
        is Route.Screen -> navigate(route.destination)
        is Route.Tab -> selectTab(route.featureId)
        is Route.Link -> openRoute(route.path)
        Route.Back -> back()
    }
}

data class AppChromeState(
    val tabId: String,
    val titleRes: Int,
    val canGoBack: Boolean = false,
    val overlay: Boolean = false,
    val hidesBottomBar: Boolean = false,
    val screenActions: List<ToolbarAction> = emptyList(),
)

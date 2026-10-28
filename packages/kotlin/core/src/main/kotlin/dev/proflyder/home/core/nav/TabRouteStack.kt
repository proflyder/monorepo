package dev.proflyder.home.core.nav

/**
 * Pure selected-tab route state used by the Fragment router.
 *
 * The backing FragmentManager has one shared back stack. Switching tabs
 * therefore resets the target tab to its root; independent per-tab Fragment
 * back stacks are deliberately not claimed. Only the selected tab stack is
 * retained so the saved bundle cannot describe routes that FragmentManager no
 * longer owns.
 */
class TabRouteStack(
    initialTabId: String,
    initialRootRoute: String,
) {
    private var selectedTab = initialTabId
    private val stack = mutableListOf(initialRootRoute)

    val currentTabId: String get() = selectedTab

    fun selectTab(tabId: String, rootRoute: String) {
        selectedTab = tabId
        stack.clear()
        stack.add(rootRoute)
    }

    /** Clears inner routes when the user re-selects a bottom tab. */
    fun resetTab(tabId: String, rootRoute: String) {
        selectTab(tabId, rootRoute)
    }

    fun currentRoute(): String = stack.last()

    fun currentStack(): List<String> = stack.toList()

    fun isAtRoot(): Boolean = stack.size == 1

    /**
     * Adds [route] unless it is already current. Returns whether a route was
     * actually pushed, so the caller never creates a second Fragment
     * transaction/back-stack entry for the same destination.
     */
    fun push(route: String): Boolean {
        if (route.isBlank() || stack.last() == route) return false
        stack.add(route)
        return true
    }

    /**
     * Pops an inner route. Returns the new current route, or null when the
     * selected tab is already at its root (the caller should exit / delegate).
     */
    fun pop(): String? {
        if (stack.size <= 1) return null
        stack.removeAt(stack.lastIndex)
        return stack.last()
    }

    fun snapshot(): Map<String, List<String>> = mapOf(selectedTab to stack.toList())

    /**
     * Restores a validated stack for [tabId]. A missing/foreign root or any
     * route outside this tab falls back to the tab root instead of leaving the
     * container blank or pointing at an unrelated feature.
     */
    fun restore(
        tabId: String,
        routes: List<String>,
        rootRoute: String,
        ownsRoute: (String) -> Boolean = { true },
    ) {
        selectedTab = tabId
        stack.clear()
        stack.add(rootRoute)
        if (routes.isEmpty() || routes.first() != rootRoute) return
        val inner = routes.drop(1)
        if (inner.any { it.isBlank() || !ownsRoute(it) }) return
        inner.forEach { route ->
            if (stack.last() != route) stack.add(route)
        }
    }
}

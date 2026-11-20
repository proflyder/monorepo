package dev.proflyder.home.core.nav

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

    fun resetTab(tabId: String, rootRoute: String) {
        selectTab(tabId, rootRoute)
    }

    fun currentRoute(): String = stack.last()

    fun currentStack(): List<String> = stack.toList()

    fun isAtRoot(): Boolean = stack.size == 1

    fun push(route: String): Boolean {
        if (route.isBlank() || stack.last() == route) return false
        stack.add(route)
        return true
    }

    fun pop(): String? {
        if (stack.size <= 1) return null
        stack.removeAt(stack.lastIndex)
        return stack.last()
    }

    fun snapshot(): Map<String, List<String>> = mapOf(selectedTab to stack.toList())

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

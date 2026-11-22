package dev.proflyder.home.core.navigation

import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentManager
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.debug
import dev.proflyder.home.core.model.warn
import dev.proflyder.home.core.nav.PendingTransitions
import dev.proflyder.home.core.nav.TabRouteStack
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

interface Navigator {
    val chrome: StateFlow<AppChromeState>

    fun selectTab(featureId: String)

    fun navigate(destination: Destination)

    fun openRoute(route: String): Boolean

    fun back(): Boolean
}

class FragmentRouter(
    private val fragmentManager: FragmentManager,
    private val containerId: Int,
    private val registry: FeatureRegistry,
    private val logger: AppLogger,
    savedState: RouterSavedState? = null,
) : Navigator, DefaultLifecycleObserver {
    private val initialEntry = registry.tabs().firstOrNull()
        ?: error("at least one bottom-bar feature is required")
    private val routeState = TabRouteStack(initialEntry.id, initialEntry.rootDestination.route)
    private val pending = PendingTransitions<Command>(capacity = 32)
    private val titlesByRoute = mutableMapOf<String, Int>()

    private var resumed = false
    private var initialized = false

    private val mutableChrome = MutableStateFlow(
        AppChromeState(tabId = initialEntry.id, titleRes = initialEntry.titleRes(initialEntry.rootDestination)),
    )
    override val chrome: StateFlow<AppChromeState> = mutableChrome.asStateFlow()

    init {
        titlesByRoute[initialEntry.rootDestination.route] = mutableChrome.value.titleRes
        if (savedState != null) restoreSavedState(savedState)
    }

    fun onHostResumed() {
        resumed = true
        if (!initialized) {
            initialized = true
            if (!restoreFromFragmentManager()) showInitialTabBeforeQueued()
        }
        drainIfPossible()
    }

    private fun showInitialTabBeforeQueued() {
        val queued = pending.snapshot()
        pending.clear()
        pending.offer(Command.SelectTab(routeState.currentTabId))
        queued.forEach(pending::offer)
    }

    fun onHostPaused() {
        resumed = false
    }

    override fun onResume(owner: LifecycleOwner) = onHostResumed()

    override fun onPause(owner: LifecycleOwner) = onHostPaused()

    override fun selectTab(featureId: String) {
        val entry = registry.entry(featureId)
        if (entry == null) {
            logger.warn(TAG, "ignoring unknown feature tab")
            return
        }
        if (!entry.isTab) {
            val host = registry.tabs().firstNotNullOfOrNull { tab ->
                tab.hostedDestination(featureId)?.let { destination -> tab to destination }
            }
            if (host != null) {
                enqueue(Command.SelectTab(host.first.id, host.second))
                return
            }
            navigate(entry.rootDestination)
            return
        }
        enqueue(Command.SelectTab(featureId, null))
    }

    override fun navigate(destination: Destination) {
        val entry = registry.entryFor(destination)
        if (entry == null || !entry.supports(destination)) {
            logger.warn(TAG, "ignoring unregistered destination")
            return
        }
        if (enqueue(Command.Push(destination))) return
        logger.warn(TAG, "navigation queue full; dropped new destination")
    }

    override fun openRoute(route: String): Boolean {
        val path = RoutePath.of(route) ?: return false
        val byId = registry.entry(path.value)
        val entry = byId ?: registry.entryForRoute(path.value) ?: return false
        val destination = (if (byId != null) entry.rootDestination else entry.deepLink(path)) ?: return false
        when {
            entry.isTab && destination.route == entry.rootDestination.route -> selectTab(entry.id)
            else -> navigate(destination)
        }
        return true
    }

    override fun back(): Boolean {
        if (!resumed || fragmentManager.isStateSaved) {
            if (!canNavigateBack()) return false
            if (!enqueue(Command.Back)) {
                logger.warn(TAG, "navigation queue full; dropped queued Back")
            }
            return true
        }
        return performBack()
    }

    fun onHostSaveState(outState: Bundle) {
        val tabId = routeState.currentTabId
        val stack = routeState.currentStack()
        outState.putString(KEY_SELECTED_TAB, tabId)
        outState.putStringArrayList(KEY_TABS, arrayListOf(tabId))
        outState.putStringArrayList("$KEY_STACK_PREFIX$tabId", ArrayList(stack))

        val titledRoutes = stack.filter { titlesByRoute.containsKey(it) }
        outState.putStringArrayList(KEY_TITLE_ROUTES, ArrayList(titledRoutes))
        outState.putIntegerArrayList(
            KEY_TITLE_VALUES,
            ArrayList(titledRoutes.map { titlesByRoute.getValue(it) }),
        )
    }

    private fun restoreSavedState(savedState: RouterSavedState) {
        val entry = registry.entry(savedState.selectedTabId)?.takeIf { it.isTab }
            ?: savedState.stacks.keys.firstNotNullOfOrNull { registry.entry(it)?.takeIf(FeatureEntry::isTab) }
            ?: initialEntry
        val savedStack = savedState.stacks[entry.id].orEmpty()
        val rootRoute = savedStack.firstOrNull()
            ?.takeIf { route -> registry.entryForRoute(route)?.id == entry.id }
            ?: entry.rootDestination.route
        routeState.restore(
            tabId = entry.id,
            routes = savedStack,
            rootRoute = rootRoute,
        ) { route ->
            val owner = registry.entryForRoute(route)
            owner != null && (owner.id == entry.id || !owner.isTab)
        }

        savedState.titles.forEach { (route, title) ->
            if (route.isNotBlank() && title != 0 && registry.entryForRoute(route) != null) {
                titlesByRoute[route] = title
            }
        }
        titlesByRoute[rootRoute] = entry.titleRes(entry.rootDestination)

        publishChrome(entry, canGoBack = routeState.currentStack().size > 1)
    }

    private fun restoreFromFragmentManager(): Boolean {
        val current = fragmentManager.findFragmentById(containerId) ?: return false
        val currentTag = current.tag
        val names = backStackNames()
        val currentEntry = currentTag?.let(registry::entryForRoute)

        if (currentTag == null || currentEntry == null) {
            resetToRoot(currentTabEntry())
            return true
        }

        val tabEntry = if (currentEntry.isTab) {
            currentEntry
        } else {
            names.filterNotNull()
                .firstNotNullOfOrNull { name -> registry.entryForRoute(name)?.takeIf { it.isTab } }
                ?: currentTabEntry()
        }
        val rootRoute = routeState.currentStack().firstOrNull()
            ?.takeIf { route ->
                routeState.currentTabId == tabEntry.id && registry.entryForRoute(route)?.id == tabEntry.id
            }
            ?: tabEntry.rootDestination.route
        val validNames = names.all { name ->
            val owner = name?.let(registry::entryForRoute) ?: return@all false
            owner.id == tabEntry.id || !owner.isTab
        }
        if (!validNames || names.lastOrNull() != currentTag) {
            resetToRoot(tabEntry)
            return true
        }

        val routes = mutableListOf(rootRoute)
        names.filterNotNull().forEach { name ->
            if (name != routes.last()) routes.add(name)
        }
        if (currentTag != routes.last()) {
            resetToRoot(tabEntry)
            return true
        }

        routeState.restore(
            tabId = tabEntry.id,
            routes = routes,
            rootRoute = rootRoute,
        ) { route ->
            val owner = registry.entryForRoute(route)
            owner != null && (owner.id == tabEntry.id || !owner.isTab)
        }
        titlesByRoute.putIfAbsent(rootRoute, tabEntry.titleRes(tabEntry.rootDestination))
        titlesByRoute.putIfAbsent(currentTag, currentEntry.titleRes)
        updateDerivedState()
        logger.debug(TAG, "restored ${routeState.currentStack()}")
        return true
    }

    private fun currentTabEntry(): FeatureEntry =
        registry.entry(routeState.currentTabId)?.takeIf { it.isTab } ?: initialEntry

    private fun backStackNames(): List<String?> =
        (0 until fragmentManager.backStackEntryCount).map { index ->
            fragmentManager.getBackStackEntryAt(index).name
        }

    private fun canNavigateBack(): Boolean =
        routeState.currentStack().size > 1 ||
            fragmentManager.backStackEntryCount > 0 ||
            pending.snapshot().any { it is Command.Push || it is Command.SelectTab }

    private fun performBack(): Boolean {
        if (fragmentManager.backStackEntryCount == 0) {
            if (routeState.isAtRoot()) return false
            if (!restoreFromFragmentManager()) {
                resetToRoot(registry.entry(routeState.currentTabId) ?: initialEntry)
            }
            return true
        }

        val popped = routeState.pop()
        fragmentManager.popBackStackImmediate()
        if (popped == null) {
            restoreFromFragmentManager()
        } else {
            updateDerivedState()
            logger.debug(TAG, "back to ${routeState.currentRoute()}")
        }
        return true
    }

    private fun enqueue(command: Command): Boolean {
        if (!pending.offer(command)) {
            logger.warn(TAG, "navigation queue full; rejected new command")
            return false
        }
        drainIfPossible()
        return true
    }

    private fun drainIfPossible() {
        if (!resumed || fragmentManager.isStateSaved || fragmentManager.isDestroyed) return
        runCatching {
            pending.drain { command ->
                if (fragmentManager.isStateSaved) throw StateSavedDuringDrain()
                execute(command)
            }
        }.onFailure { failure ->
            if (failure !is StateSavedDuringDrain) throw failure
        }
    }

    private fun execute(command: Command) {
        when (command) {
            is Command.SelectTab -> selectTabNow(command.featureId, command.destination)
            is Command.Push -> pushNow(command.destination)
            Command.Back -> performBack()
        }
    }

    private fun selectTabNow(featureId: String, destination: Destination? = null) {
        val entry = registry.entry(featureId) ?: return
        val root = destination ?: entry.rootDestination
        val alreadyAtRoot = routeState.currentTabId == entry.id &&
            routeState.currentRoute() == root.route &&
            routeState.isAtRoot() &&
            fragmentManager.backStackEntryCount == 0 &&
            fragmentManager.findFragmentByTag(root.route) != null

        routeState.selectTab(entry.id, root.route)
        titlesByRoute[root.route] = entry.titleRes(root)
        if (!alreadyAtRoot) {
            popEverything()
            replaceFragment(root, addToBackStack = false)
        }
        updateDerivedState()
    }

    private fun pushNow(destination: Destination) {
        val entry = registry.entryFor(destination) ?: return
        if (!entry.isTab) {
            if (!routeState.push(destination.route)) {
                updateDerivedState()
                return
            }
            titlesByRoute[destination.route] = entry.titleRes(destination)
            replaceFragment(destination, addToBackStack = true)
            updateDerivedState()
            return
        }
        if (routeState.currentTabId != entry.id) {
            routeState.selectTab(entry.id, entry.rootDestination.route)
            titlesByRoute[entry.rootDestination.route] = entry.titleRes(entry.rootDestination)
            popEverything()
            replaceFragment(entry.rootDestination, addToBackStack = false)
        }

        if (!routeState.push(destination.route)) {
            updateDerivedState()
            return
        }

        titlesByRoute[destination.route] = entry.titleRes(destination)
        replaceFragment(destination, addToBackStack = true)
        updateDerivedState()
    }

    private fun resetToRoot(entry: FeatureEntry) {
        val root = entry.rootDestination
        popEverything()
        routeState.restore(
            tabId = entry.id,
            routes = listOf(root.route),
            rootRoute = root.route,
        )
        titlesByRoute[root.route] = entry.titleRes(root)
        replaceFragment(root, addToBackStack = false)
        updateDerivedState()
    }

    private fun popEverything() {
        if (fragmentManager.backStackEntryCount > 0) {
            fragmentManager.popBackStackImmediate(null, FragmentManager.POP_BACK_STACK_INCLUSIVE)
        }
    }

    private fun replaceFragment(destination: Destination, addToBackStack: Boolean) {
        val entry = registry.entryFor(destination) ?: return
        val fragment: Fragment = entry.createFragment(destination)
        fragment.arguments = RouteArgs.bundle(destination)
        val transaction = fragmentManager.beginTransaction()
            .setReorderingAllowed(true)
            .replace(containerId, fragment, destination.route)
        if (addToBackStack) transaction.addToBackStack(destination.route)
        transaction.commit()
        fragmentManager.executePendingTransactions()
    }

    private fun updateDerivedState() {
        val entry = registry.entry(routeState.currentTabId) ?: initialEntry
        publishChrome(entry, canGoBack = fragmentManager.backStackEntryCount > 0)
    }

    private fun publishChrome(tab: FeatureEntry, canGoBack: Boolean) {
        val routeEntry = registry.entryForRoute(routeState.currentRoute())
        mutableChrome.value = AppChromeState(
            tabId = tab.id,
            titleRes = titlesByRoute[routeState.currentRoute()] ?: tab.titleRes(tab.rootDestination),
            canGoBack = canGoBack,
            overlay = currentRouteOverlay(),
            hidesBottomBar = routeEntry?.hidesBottomBar == true,
        )
    }

    private fun currentRouteOverlay(): Boolean {
        val entry = registry.entryForRoute(routeState.currentRoute()) ?: return false
        return !entry.isTab
    }

    private sealed interface Command {
        data class SelectTab(val featureId: String, val destination: Destination? = null) : Command
        data class Push(val destination: Destination) : Command
        data object Back : Command
    }

    private class StateSavedDuringDrain : RuntimeException()

    data class RouterSavedState(
        val selectedTabId: String,
        val stacks: Map<String, List<String>>,
        val titles: Map<String, Int> = emptyMap(),
    ) {
        companion object {
            fun from(bundle: Bundle?): RouterSavedState? {
                if (bundle == null) return null
                val tabs = bundle.getStringArrayList(KEY_TABS).orEmpty()
                val selected = bundle.getString(KEY_SELECTED_TAB) ?: tabs.firstOrNull() ?: return null
                val stacks = tabs.associateWith { tab ->
                    bundle.getStringArrayList("$KEY_STACK_PREFIX$tab").orEmpty()
                }.filterValues { it.isNotEmpty() }

                val titleRoutes = bundle.getStringArrayList(KEY_TITLE_ROUTES).orEmpty()
                val titleValues = bundle.getIntegerArrayList(KEY_TITLE_VALUES).orEmpty()
                val titles = titleRoutes.zip(titleValues)
                    .filter { (route, title) -> route.isNotBlank() && title != 0 }
                    .toMap()

                return RouterSavedState(selected, stacks, titles)
            }
        }
    }

    companion object {
        private const val TAG = "FragmentRouter"
        private const val KEY_SELECTED_TAB = "router_selected_tab"
        private const val KEY_TABS = "router_tabs"
        private const val KEY_STACK_PREFIX = "router_stack_"
        private const val KEY_TITLE_ROUTES = "router_title_routes"
        private const val KEY_TITLE_VALUES = "router_title_values"
    }
}

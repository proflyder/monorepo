package dev.proflyder.home.core.navigation

import android.os.Bundle
import android.util.Log
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentManager
import dev.proflyder.home.core.nav.PendingTransitions
import dev.proflyder.home.core.nav.TabRouteStack
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Public navigation surface consumed by the Activity and features. */
interface Navigator {
    val currentTabId: StateFlow<String>
    val currentTitleRes: StateFlow<Int>
    val canGoBack: StateFlow<Boolean>

    fun selectTab(featureId: String)

    fun navigate(destination: Destination)

    /** Returns false when the current tab is at its root (Activity should exit). */
    fun back(): Boolean
}

/**
 * Small typed Fragment router.
 *
 * - FragmentManager owns the real back stack and restores fragments after
 *   process death. The restored fragment/back stack is authoritative; saved
 *   route strings are validated against it instead of recreating it blindly.
 * - Commands emitted while the host is paused or FragmentManager has saved
 *   state are queued and drained only after onResume. This includes Back.
 * - Switching bottom tabs resets the target tab to its root. There is one
 *   shared Fragment back stack, so independent per-tab stacks are not claimed.
 * - Only route strings, tab ids, and resolved title resources are saved; never
 *   tokens or endpoints.
 */
class FragmentRouter(
    private val fragmentManager: FragmentManager,
    private val containerId: Int,
    private val registry: FeatureRegistry,
    savedState: RouterSavedState? = null,
) : Navigator {
    private val initialEntry = registry.all().first()
    private val routeState = TabRouteStack(initialEntry.id, initialEntry.rootDestination.route)
    private val pending = PendingTransitions<Command>(capacity = 32)
    private val titlesByRoute = mutableMapOf<String, Int>()

    private var resumed = false
    private var initialized = false

    private val mutableTab = MutableStateFlow(initialEntry.id)
    override val currentTabId: StateFlow<String> = mutableTab.asStateFlow()

    private val mutableTitle = MutableStateFlow(initialEntry.titleRes)
    override val currentTitleRes: StateFlow<Int> = mutableTitle.asStateFlow()

    private val mutableCanGoBack = MutableStateFlow(false)
    override val canGoBack: StateFlow<Boolean> = mutableCanGoBack.asStateFlow()

    init {
        mutableTitle.value = initialEntry.titleRes(initialEntry.rootDestination)
        titlesByRoute[initialEntry.rootDestination.route] = mutableTitle.value
        if (savedState != null) restoreSavedState(savedState)
    }

    fun onHostResumed() {
        resumed = true
        if (!initialized) {
            initialized = true
            if (!restoreFromFragmentManager()) {
                // No actual restored fragment. A saved bundle alone must not
                // leave the container blank: rebuild the selected tab root.
                enqueue(Command.SelectTab(routeState.currentTabId))
            }
        }
        drainIfPossible()
    }

    fun onHostPaused() {
        resumed = false
    }

    override fun selectTab(featureId: String) {
        val entry = registry.entry(featureId)
        if (entry == null) {
            Log.w(TAG, "ignoring unknown feature tab")
            return
        }
        enqueue(Command.SelectTab(featureId))
    }

    override fun navigate(destination: Destination) {
        val entry = registry.entryFor(destination)
        if (entry == null || !entry.supports(destination)) {
            Log.w(TAG, "ignoring unregistered destination")
            return
        }
        if (enqueue(Command.Push(destination))) return
        Log.w(TAG, "navigation queue full; dropped new destination")
    }

    override fun back(): Boolean {
        if (!resumed || fragmentManager.isStateSaved) {
            // Do not silently consume Back while the host cannot run a
            // transaction: queue it and drain after resume. At the real root
            // return false so the Activity can finish instead of swallowing
            // future back presses forever.
            if (!canNavigateBack()) return false
            if (!enqueue(Command.Back)) {
                Log.w(TAG, "navigation queue full; dropped queued Back")
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

        // Destination objects are typed and not parcelable; remember the
        // resolved toolbar title for routes whose fragment FragmentManager
        // restores. Root titles are recomputed from the live FeatureEntry.
        val titledRoutes = stack.filter { titlesByRoute.containsKey(it) }
        outState.putStringArrayList(KEY_TITLE_ROUTES, ArrayList(titledRoutes))
        outState.putIntegerArrayList(
            KEY_TITLE_VALUES,
            ArrayList(titledRoutes.map { titlesByRoute.getValue(it) }),
        )
    }

    private fun restoreSavedState(savedState: RouterSavedState) {
        val entry = registry.entry(savedState.selectedTabId)
            ?: savedState.stacks.keys.firstNotNullOfOrNull(registry::entry)
            ?: initialEntry
        val rootRoute = entry.rootDestination.route
        routeState.restore(
            tabId = entry.id,
            routes = savedState.stacks[entry.id].orEmpty(),
            rootRoute = rootRoute,
        ) { route -> registry.entryForRoute(route)?.id == entry.id }

        savedState.titles.forEach { (route, title) ->
            if (route.isNotBlank() && title != 0 && registry.entryForRoute(route) != null) {
                titlesByRoute[route] = title
            }
        }
        titlesByRoute[rootRoute] = entry.titleRes(entry.rootDestination)

        mutableTab.value = entry.id
        mutableTitle.value = titlesByRoute[routeState.currentRoute()] ?: entry.titleRes(entry.rootDestination)
        mutableCanGoBack.value = routeState.currentStack().size > 1
    }

    /**
     * Reconciles route state with the actual restored FragmentManager. Returns
     * false only when the container has no fragment and a root commit is
     * required.
     */
    private fun restoreFromFragmentManager(): Boolean {
        val current = fragmentManager.findFragmentById(containerId) ?: return false
        val currentTag = current.tag
        val names = backStackNames()
        val entry = currentTag?.let(registry::entryForRoute)

        if (currentTag == null || entry == null) {
            // An unknown/stale fragment must not be treated as route truth.
            resetToRoot(registry.entry(routeState.currentTabId) ?: initialEntry)
            return true
        }

        val rootRoute = entry.rootDestination.route
        if (names.isEmpty()) {
            if (currentTag != rootRoute) {
                resetToRoot(entry)
                return true
            }
        } else {
            val validNames = names.all { name ->
                !name.isNullOrBlank() && registry.entryForRoute(name)?.id == entry.id
            }
            if (!validNames || names.last() != currentTag) {
                resetToRoot(entry)
                return true
            }
        }

        val routes = mutableListOf(rootRoute)
        names.filterNotNull().forEach { name ->
            if (name != routes.last()) routes.add(name)
        }
        if (currentTag != routes.last()) {
            resetToRoot(entry)
            return true
        }

        routeState.restore(
            tabId = entry.id,
            routes = routes,
            rootRoute = rootRoute,
        ) { route -> registry.entryForRoute(route)?.id == entry.id }
        titlesByRoute.putIfAbsent(rootRoute, entry.titleRes(entry.rootDestination))
        // The saved title for an editor was loaded above; this is only a safe
        // fallback for an old/corrupt bundle.
        titlesByRoute.putIfAbsent(currentTag, entry.titleRes)
        updateDerivedState()
        Log.d(TAG, "restored ${routeState.currentStack()}")
        return true
    }

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
            // Route state and FragmentManager diverged. Re-sync once and consume
            // this Back, but do not trap the user at a phantom inner route.
            if (!restoreFromFragmentManager()) {
                resetToRoot(registry.entry(routeState.currentTabId) ?: initialEntry)
            }
            return true
        }

        val popped = routeState.pop()
        fragmentManager.popBackStackImmediate()
        if (popped == null) {
            // Route state did not own the entry FragmentManager had; rebuild
            // from the restored fragment instead of leaving the two out of sync.
            restoreFromFragmentManager()
        } else {
            updateDerivedState()
            Log.d(TAG, "back to ${routeState.currentRoute()}")
        }
        return true
    }

    private fun enqueue(command: Command): Boolean {
        if (!pending.offer(command)) {
            // Preserve already accepted commands. Reject only the new command;
            // clearing the queue would silently lose user navigation.
            Log.w(TAG, "navigation queue full; rejected new command")
            return false
        }
        drainIfPossible()
        return true
    }

    private fun drainIfPossible() {
        if (!resumed || fragmentManager.isStateSaved || fragmentManager.isDestroyed) return
        try {
            pending.drain { command ->
                // State may have been saved between commands (e.g. an Activity
                // transition); stop and keep the rest for the next resume.
                if (fragmentManager.isStateSaved) throw StateSavedDuringDrain()
                execute(command)
            }
        } catch (_: StateSavedDuringDrain) {
            // Remaining commands stay queued for the next onHostResumed().
        }
    }

    private fun execute(command: Command) {
        when (command) {
            is Command.SelectTab -> selectTabNow(command.featureId)
            is Command.Push -> pushNow(command.destination)
            Command.Back -> performBack()
        }
    }

    private fun selectTabNow(featureId: String) {
        val entry = registry.entry(featureId) ?: return
        val root = entry.rootDestination
        val alreadyAtRoot = routeState.currentTabId == entry.id &&
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
        if (routeState.currentTabId != entry.id) {
            routeState.selectTab(entry.id, entry.rootDestination.route)
            titlesByRoute[entry.rootDestination.route] = entry.titleRes(entry.rootDestination)
            popEverything()
            replaceFragment(entry.rootDestination, addToBackStack = false)
        }

        if (!routeState.push(destination.route)) {
            // Same destination is already current: no second Fragment
            // transaction and no second back-stack entry (double tap safe).
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
        fragment.arguments = Bundle(destination.arguments())
        val transaction = fragmentManager.beginTransaction()
            .setReorderingAllowed(true)
            .replace(containerId, fragment, destination.route)
        if (addToBackStack) transaction.addToBackStack(destination.route)
        transaction.commit()
        // Keep route state, canGoBack, and the real Fragment back stack in step;
        // callers are gated on a resumed, non-state-saved host.
        fragmentManager.executePendingTransactions()
    }

    private fun updateDerivedState() {
        val entry = registry.entry(routeState.currentTabId) ?: initialEntry
        mutableTab.value = entry.id
        mutableTitle.value = titlesByRoute[routeState.currentRoute()]
            ?: entry.titleRes(entry.rootDestination)
        mutableCanGoBack.value = fragmentManager.backStackEntryCount > 0
    }

    private sealed interface Command {
        data class SelectTab(val featureId: String) : Command
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

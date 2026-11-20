package dev.proflyder.home

import android.content.Intent
import android.os.Bundle
import android.view.Menu
import android.view.MenuItem
import android.view.View
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.isVisible
import androidx.core.view.updatePadding
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.bottomnavigation.BottomNavigationView
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.FragmentRouter
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.NavigatorHost
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.AppForeground
import dev.proflyder.home.core.ui.applyNavigationInset
import dev.proflyder.home.core.ui.doOnApplyInsets
import dev.proflyder.home.core.ui.forContentBetweenBars
import dev.proflyder.home.core.ui.horizontalInsets
import dev.proflyder.home.core.ui.keyboardVisible
import dev.proflyder.home.core.ui.topInset
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.databinding.ActivityMainBinding
import dev.proflyder.home.feature.control.presentation.ControlUiState
import dev.proflyder.home.feature.control.presentation.ControlViewModel
import dev.proflyder.home.feature.push.PushIntents
import dev.proflyder.home.feature.settings.SettingsDestination
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

class MainActivity : AppCompatActivity(), NavigatorHost, ViewModelFactoryProvider {
    internal lateinit var router: FragmentRouter
    private lateinit var binding: ActivityMainBinding
    private lateinit var features: FeatureRegistry
    internal lateinit var actionsMenu: ToolbarActionsMenu

    private val bottomNavigation: BottomNavigationView get() = binding.homeNoticeAnchor
    private val toolbar: MaterialToolbar get() = binding.toolbar

    /** The same activity-scoped ViewModel the Home fragment uses. */
    private val controlViewModel: ControlViewModel by viewModels {
        viewModelFactory(ControlViewModel::class.java)
    }

    private val itemIdByFeature = mutableMapOf<String, Int>()
    private val featureByItemId = mutableMapOf<Int, String>()
    private var syncingSelection = false

    /** Elapsed time when the activity left the foreground; 0 before the first start. */
    private var leftForegroundAtNanos = 0L

    private val graph get() = (application as HomeApplication).graph

    override val navigator: Navigator get() = router

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val graph = (application as HomeApplication).graph
        features = graph.featureRegistry
        enableEdgeToEdge()
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
        actionsMenu = ToolbarActionsMenu(this, ::runMenuAction)
        applySystemBarInsets()

        // FragmentRouter inspects the actual restored FragmentManager state;
        // a non-null savedInstanceState alone does not prove a fragment exists.
        router = FragmentRouter(
            fragmentManager = supportFragmentManager,
            containerId = R.id.fragment_container,
            registry = features,
            savedState = FragmentRouter.RouterSavedState.from(savedInstanceState),
        )
        setupBottomNavigation()
        setupToolbar()
        applyDeepLink(intent)

        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    if (!router.back()) finish()
                }
            },
        )

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { observeAppState() }
        }
    }

    private suspend fun CoroutineScope.observeAppState() {
        launch {
            router.currentTabId.collect(::syncBottomSelection)
        }
        launch {
            router.currentTitleRes.collect { binding.toolbarTitle.setText(it) }
        }
        launch {
            controlViewModel.state.collect {
                renderProfileChip(it, router.currentTabId.value, router.currentIsOverlay.value)
            }
        }
        launch {
            router.currentTabId.collect {
                renderProfileChip(controlViewModel.state.value, it, router.currentIsOverlay.value)
            }
        }
        launch {
            router.currentIsOverlay.collect(::renderToolbarMode)
        }
        launch {
            router.canGoBack.collect { canGoBack ->
                if (canGoBack) {
                    toolbar.setNavigationIcon(R.drawable.app_ic_back)
                    toolbar.setNavigationContentDescription(R.string.app_back)
                } else {
                    toolbar.navigationIcon = null
                }
            }
        }
    }

    override fun onDestroy() {
        if (::actionsMenu.isInitialized) actionsMenu.dismiss()
        super.onDestroy()
    }

    override fun onResume() {
        super.onResume()
        router.onHostResumed()
    }

    override fun onStart() {
        super.onStart()
        val leftAt = leftForegroundAtNanos
        when {
            leftAt == 0L -> Unit
            System.nanoTime() - leftAt >= STALE_CONNECTION_NANOS -> graph.httpClient.connectionPool.evictAll()
            else -> Unit
        }
        AppForeground.onResumed()
    }

    override fun onStop() {
        leftForegroundAtNanos = System.nanoTime()
        super.onStop()
    }

    override fun onPause() {
        router.onHostPaused()
        super.onPause()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        applyDeepLink(intent)
    }

    /**
     * A notification tap carries an in-app route (`home://alarms`). The router
     * accepts only registered features and owned routes; an unknown or foreign
     * route just opens the app on the current tab.
     */
    private fun applyDeepLink(intent: Intent?) {
        val route = intent?.getStringExtra(PushIntents.EXTRA_DEEP_LINK)?.trim().orEmpty()
        if (route.isEmpty()) return
        intent?.removeExtra(PushIntents.EXTRA_DEEP_LINK)
        router.openRoute(route)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        router.onHostSaveState(outState)
        super.onSaveInstanceState(outState)
    }

    override fun viewModelFactory(viewModelClass: Class<out ViewModel>): ViewModelProvider.Factory =
        (application as HomeApplication).graph.viewModelFactories.viewModelFactory(viewModelClass)

    private fun syncBottomSelection(featureId: String) {
        val itemId = itemIdByFeature[featureId] ?: return
        if (bottomNavigation.selectedItemId == itemId) return
        syncingSelection = true
        bottomNavigation.selectedItemId = itemId
        syncingSelection = false
    }

    private fun setupBottomNavigation() {
        val menu: Menu = bottomNavigation.menu
        menu.clear()
        features.tabs().forEachIndexed { index, entry -> addTab(menu, index + 1, entry) }
        // Restore the selected tab before attaching the listener so the menu's
        // automatic first-item selection cannot override router state.
        itemIdByFeature[router.currentTabId.value]?.let { bottomNavigation.selectedItemId = it }
        bottomNavigation.setOnItemSelectedListener { item ->
            if (!syncingSelection) {
                featureByItemId[item.itemId]?.let(router::selectTab)
            }
            true
        }
    }

    private fun addTab(menu: Menu, itemId: Int, entry: FeatureEntry) {
        menu.add(Menu.NONE, itemId, entry.order, entry.titleRes).setIcon(entry.iconRes)
        itemIdByFeature[entry.id] = itemId
        featureByItemId[itemId] = entry.id
    }

    private fun setupToolbar() {
        toolbar.title = null
        toolbar.inflateMenu(R.menu.menu_main)
        // One actions button; its spoken label comes from resources. The menu
        // it opens is our own card (ToolbarActionsMenu), not an overflow list.
        toolbar.menu.findItem(R.id.action_menu)?.contentDescription = getString(R.string.app_actions)
        toolbar.setNavigationOnClickListener { onBackPressedDispatcher.onBackPressed() }
        toolbar.setOnMenuItemClickListener(::onToolbarMenuItem)
    }

    private fun onToolbarMenuItem(item: MenuItem): Boolean = when (item.itemId) {
        R.id.action_menu -> {
            showActionsMenu()
            true
        }
        else -> false
    }

    private fun showActionsMenu() {
        val anchor = toolbar.findViewById<View>(R.id.action_menu) ?: return
        actionsMenu.toggle(anchor)
    }

    private fun runMenuAction(actionId: Int) {
        when (actionId) {
            R.id.menu_action_call -> (application as HomeApplication).graph.callLauncher.openCallScreen()
            R.id.menu_action_settings -> router.navigate(SettingsDestination)
            else -> Unit
        }
    }

    /**
     * Overlays (Settings and the screens it opens) keep only Back and the
     * title: the profile is switched from the main tabs, and the actions menu
     * is hidden so the toolbar does not compete with the screen itself.
     */
    private fun renderToolbarMode(overlay: Boolean) {
        toolbar.menu.findItem(R.id.action_menu)?.isVisible = !overlay
        actionsMenu.dismiss()
        renderProfileChip(controlViewModel.state.value, router.currentTabId.value, overlay)
    }

    /**
     * The profile is a mode, not a list card: a compact chip next to the Home
     * title shows the current side and toggles it. It exists only on the Home
     * tab and only while the catalog exposes the profile switch.
     */
    private fun renderProfileChip(state: ControlUiState, tabId: String, overlay: Boolean) {
        val profile = state.entries.firstOrNull { it.isProfile }
        val chip = binding.profileChip
        chip.visibility = if (tabId == CONTROL_TAB && !overlay && profile != null) View.VISIBLE else View.GONE
        profile ?: return
        val work = profile.power == PowerState.ON
        chip.setText(if (work) R.string.app_profile_work else R.string.app_profile_personal)
        chip.contentDescription = getString(R.string.app_profile_chip, chip.text)
        chip.isEnabled = profile.available
        chip.setOnClickListener { controlViewModel.setProfile(profile, !work) }
    }

    private fun applySystemBarInsets() {
        binding.appbar.doOnApplyInsets { view, insets, initial ->
            val sides = insets.horizontalInsets()
            view.updatePadding(left = sides.left, top = initial.top + insets.topInset(), right = sides.right)
        }
        ViewCompat.setOnApplyWindowInsetsListener(binding.root) { _, insets ->
            bottomNavigation.isVisible = !insets.keyboardVisible()
            insets
        }
        bottomNavigation.applyNavigationInset()
        ViewCompat.setOnApplyWindowInsetsListener(binding.fragmentContainer) { view, insets ->
            val sides = insets.horizontalInsets()
            view.updatePadding(left = sides.left, right = sides.right)
            insets.forContentBetweenBars(bottomCovered = !insets.keyboardVisible())
        }
    }

    private companion object {
        const val CONTROL_TAB = "control"

        /**
         * A socket that survived longer than this in the background is almost
         * certainly dead after Doze; dropping the pool makes the first resume
         * request open a fresh connection instead of failing and retrying.
         */
        const val STALE_CONNECTION_NANOS = 15_000_000_000L
    }
}

package dev.proflyder.home

import android.content.Intent
import android.os.Bundle
import android.view.Menu
import android.view.View
import androidx.activity.OnBackPressedCallback
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
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
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.databinding.ActivityMainBinding
import dev.proflyder.home.feature.connection.ConnectionDestination
import dev.proflyder.home.feature.push.PushIntents
import dev.proflyder.home.feature.push.PushSettingsDestination
import dev.proflyder.home.feature.control.presentation.ControlUiState
import dev.proflyder.home.feature.control.presentation.ControlViewModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

class MainActivity : AppCompatActivity(), NavigatorHost, ViewModelFactoryProvider {
    internal lateinit var router: FragmentRouter
    private lateinit var binding: ActivityMainBinding
    private lateinit var features: FeatureRegistry

    private val bottomNavigation: BottomNavigationView get() = binding.homeNoticeAnchor
    private val toolbar: MaterialToolbar get() = binding.toolbar

    /** The same activity-scoped ViewModel the Home fragment uses. */
    private val controlViewModel: ControlViewModel by viewModels {
        viewModelFactory(ControlViewModel::class.java)
    }

    private val itemIdByFeature = mutableMapOf<String, Int>()
    private val featureByItemId = mutableMapOf<Int, String>()
    private var syncingSelection = false

    override val navigator: Navigator get() = router

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val graph = (application as HomeApplication).graph
        features = graph.featureRegistry
        WindowCompat.setDecorFitsSystemWindows(window, false)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
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
            controlViewModel.state.collect { renderProfileChip(it, router.currentTabId.value) }
        }
        launch {
            router.currentTabId.collect { renderProfileChip(controlViewModel.state.value, it) }
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

    override fun onResume() {
        super.onResume()
        router.onHostResumed()
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
     * A notification tap carries an in-app route (`home://alarms`). Only a
     * registered feature id is honoured: an unknown or foreign route just opens
     * the app on the current tab.
     */
    private fun applyDeepLink(intent: Intent?) {
        val route = intent?.getStringExtra(PushIntents.EXTRA_DEEP_LINK)?.trim().orEmpty()
        if (route.isEmpty()) return
        intent?.removeExtra(PushIntents.EXTRA_DEEP_LINK)
        if (features.entry(route) != null) router.selectTab(route)
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
        toolbar.setNavigationOnClickListener { onBackPressedDispatcher.onBackPressed() }
        toolbar.setOnMenuItemClickListener { item ->
            when (item.itemId) {
                R.id.action_settings -> {
                    // Settings opens as an overlay over the current tab, so Back
                    // returns to that tab instead of leaving the app.
                    router.navigate(ConnectionDestination)
                    true
                }
                R.id.action_push -> {
                    // Notifications and find-phone permissions; also an overlay.
                    router.navigate(PushSettingsDestination)
                    true
                }
                R.id.action_call -> {
                    (application as HomeApplication).graph.callLauncher.openCallScreen()
                    true
                }
                else -> false
            }
        }
    }

    /**
     * The profile is a mode, not a list card: a compact chip next to the Home
     * title shows the current side and toggles it. It exists only on the Home
     * tab and only while the catalog exposes the profile switch.
     */
    private fun renderProfileChip(state: ControlUiState, tabId: String) {
        val profile = state.entries.firstOrNull { it.isProfile }
        val chip = binding.profileChip
        chip.visibility = if (tabId == CONTROL_TAB && profile != null) View.VISIBLE else View.GONE
        profile ?: return
        val work = profile.power == PowerState.ON
        chip.setText(if (work) R.string.app_profile_work else R.string.app_profile_personal)
        chip.contentDescription = getString(R.string.app_profile_chip, chip.text)
        chip.isEnabled = profile.available
        chip.setOnClickListener { controlViewModel.setProfile(profile, !work) }
    }

    private fun applySystemBarInsets() {
        val appBar = binding.appbar
        ViewCompat.setOnApplyWindowInsetsListener(appBar) { view, insets ->
            val top = insets.getInsets(WindowInsetsCompat.Type.statusBars()).top
            view.updatePadding(top = top)
            insets
        }
        ViewCompat.setOnApplyWindowInsetsListener(bottomNavigation) { view, insets ->
            val bottom = insets.getInsets(WindowInsetsCompat.Type.navigationBars()).bottom
            view.updatePadding(bottom = bottom)
            insets
        }
        val content = binding.fragmentContainer
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            view.updatePadding(bottom = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom)
            insets
        }
    }

    private companion object {
        const val CONTROL_TAB = "control"
    }
}

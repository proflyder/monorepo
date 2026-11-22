package dev.proflyder.home

import android.content.Intent
import android.os.Bundle
import android.view.Menu
import android.view.MenuItem
import android.view.View
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
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
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.FragmentRouter
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.NavigatorHost
import dev.proflyder.home.core.ui.FailureNoticePolicy
import dev.proflyder.home.core.ui.FailureNoticePolicyProvider
import dev.proflyder.home.core.ui.applyNavigationInset
import dev.proflyder.home.core.ui.doOnApplyInsets
import dev.proflyder.home.core.ui.forContentBetweenBars
import dev.proflyder.home.core.ui.horizontalInsets
import dev.proflyder.home.core.ui.keyboardVisible
import dev.proflyder.home.core.ui.topInset
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.databinding.ActivityMainBinding
import dev.proflyder.home.feature.control.VoiceProfileState
import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.ChatRoute
import dev.proflyder.home.feature.push.PushDeepLink
import dev.proflyder.home.feature.push.PushIntents
import dev.proflyder.home.feature.push.PushPayloads
import dev.proflyder.home.feature.settings.SettingsDestination
import dev.proflyder.home.feature.vpn.VpnDestination
import dev.proflyder.home.feature.vpn.presentation.VpnActivityHost
import dev.proflyder.home.feature.vpn.impl.R as VpnR
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

class MainActivity : AppCompatActivity(), NavigatorHost, ViewModelFactoryProvider, FailureNoticePolicyProvider {
    internal lateinit var router: FragmentRouter
    private lateinit var binding: ActivityMainBinding
    private lateinit var features: FeatureRegistry
    internal lateinit var actionsMenu: ToolbarActionsMenu
    private lateinit var vpnHost: VpnActivityHost

    private val bottomNavigation: BottomNavigationView get() = binding.homeNoticeAnchor
    private val toolbar: MaterialToolbar get() = binding.toolbar
    private val vpnToggle: MaterialButton get() = binding.root.findViewById(VpnR.id.vpn_toolbar_toggle)
    private val vpnBanner: View get() = binding.root.findViewById(VpnR.id.vpn_route_banner)
    private var keyboardShown = false

    private val itemIdByFeature = mutableMapOf<String, Int>()
    private val featureByItemId = mutableMapOf<Int, String>()
    private var syncingSelection = false

    private var leftForegroundAtNanos = 0L

    private val graph get() = (application as HomeApplication).graph

    override val failureNoticePolicy: FailureNoticePolicy get() = graph.failureNoticePolicy

    override val navigator: Navigator get() = router

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val graph = (application as HomeApplication).graph
        features = graph.featureRegistry
        enableEdgeToEdge()
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
        actionsMenu = ToolbarActionsMenu(::runMenuAction)
        applySystemBarInsets()

        router = FragmentRouter(
            fragmentManager = supportFragmentManager,
            containerId = R.id.fragment_container,
            registry = features,
            savedState = FragmentRouter.RouterSavedState.from(savedInstanceState),
        )
        setupBottomNavigation()
        setupToolbar()
        vpnHost = VpnActivityHost(this) { router.navigate(VpnDestination) }
        vpnHost.bind(vpnToggle)
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
        vpnHost.observe(this, vpnToggle, vpnBanner)
        launch {
            router.currentHidesBottomBar.collect { onBottomBarChanged() }
        }
        launch {
            router.currentTabId.collect(::syncBottomSelection)
        }
        launch {
            router.currentTitleRes.collect { binding.toolbarTitle.setText(it) }
        }
        launch {
            combine(graph.voiceProfile.profile, router.currentTabId, router.currentIsOverlay, ::ProfileChip)
                .collect(::renderProfileChip)
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

    private fun applyDeepLink(intent: Intent?) {
        val route = deepLinkRoute(intent)
        if (route.isEmpty()) return
        intent?.removeExtra(PushIntents.EXTRA_DEEP_LINK)
        intent?.removeExtra(PushPayloads.KEY_DEEP_LINK)
        ChatRoute.messageId(route)?.let(ChatFocus::request)
        router.openRoute(route)
    }

    private fun deepLinkRoute(intent: Intent?): String {
        val direct = intent?.getStringExtra(PushIntents.EXTRA_DEEP_LINK)?.trim().orEmpty()
        val fromTray = PushDeepLink.parse(intent?.getStringExtra(PushPayloads.KEY_DEEP_LINK))?.route.orEmpty()
        return direct.ifEmpty { fromTray }
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
        HomePopupMenu.markObstacle(bottomNavigation)
        val menu: Menu = bottomNavigation.menu
        menu.clear()
        features.tabs().forEachIndexed { index, entry -> addTab(menu, index + 1, entry) }
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

    private fun renderToolbarMode(overlay: Boolean) {
        toolbar.menu.findItem(R.id.action_menu)?.isVisible = !overlay
        vpnToggle.isVisible = !overlay
        actionsMenu.dismiss()
    }

    private fun renderProfileChip(state: ProfileChip) {
        val profile = state.profile
        val chip = binding.profileChip
        chip.visibility = if (state.tabId == CONTROL_TAB && !state.overlay && profile != null) View.VISIBLE else View.GONE
        profile ?: return
        chip.setText(if (profile.work) R.string.app_profile_work else R.string.app_profile_personal)
        chip.contentDescription = getString(R.string.app_profile_chip, chip.text)
        chip.isEnabled = profile.available
        chip.setOnClickListener { lifecycleScope.launch { graph.voiceProfile.setWork(!profile.work) } }
    }

    private data class ProfileChip(val profile: VoiceProfileState?, val tabId: String, val overlay: Boolean)

    private fun applySystemBarInsets() {
        binding.appbar.doOnApplyInsets { view, insets, initial ->
            val sides = insets.horizontalInsets()
            view.updatePadding(left = sides.left, top = initial.top + insets.topInset(), right = sides.right)
        }
        ViewCompat.setOnApplyWindowInsetsListener(binding.root) { _, insets ->
            keyboardShown = insets.keyboardVisible()
            renderBottomBar()
            insets
        }
        bottomNavigation.applyNavigationInset()
        ViewCompat.setOnApplyWindowInsetsListener(binding.fragmentContainer) { view, insets ->
            val sides = insets.horizontalInsets()
            view.updatePadding(left = sides.left, right = sides.right)
            insets.forContentBetweenBars(bottomCovered = !insets.keyboardVisible() && !router.currentHidesBottomBar.value)
        }
    }

    private fun onBottomBarChanged() {
        renderBottomBar()
        ViewCompat.requestApplyInsets(binding.root)
    }

    private fun renderBottomBar() {
        bottomNavigation.isVisible = !keyboardShown && !router.currentHidesBottomBar.value
    }

    private companion object {
        const val CONTROL_TAB = "control"

        const val STALE_CONNECTION_NANOS = 15_000_000_000L
    }
}

package dev.proflyder.home

import android.content.Intent
import android.os.Bundle
import androidx.activity.addCallback
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.isVisible
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.proflyder.home.core.access.AppAccessSession
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.designsystem.glass.AppBarHost
import dev.proflyder.home.core.designsystem.glass.GlassChrome
import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.navigation.AppActivityContribution
import dev.proflyder.home.core.navigation.AppChromeState
import dev.proflyder.home.core.navigation.DeepLinkResolver
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.FragmentRouter
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.NavigatorHost
import dev.proflyder.home.core.navigation.ToolbarAction
import dev.proflyder.home.core.navigation.ToolbarContribution
import dev.proflyder.home.core.navigation.ToolbarSlots
import dev.proflyder.home.core.ui.FailureNoticePolicy
import dev.proflyder.home.core.ui.FailureNoticePolicyProvider
import dev.proflyder.home.databinding.ActivityMainBinding
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

class MainActivity :
    AppCompatActivity(),
    NavigatorHost,
    FailureNoticePolicyProvider,
    AppBarHost {
    override val glassChrome = GlassChrome()

    internal lateinit var router: FragmentRouter
    internal lateinit var actionsMenu: ToolbarChrome
    private lateinit var binding: ActivityMainBinding
    private lateinit var tabs: BottomTabs

    @Inject
    internal lateinit var features: FeatureRegistry

    @Inject
    override lateinit var failureNoticePolicy: FailureNoticePolicy

    @Inject
    internal lateinit var toolbarContributions: Set<ToolbarContribution>

    @Inject
    internal lateinit var toolbarActions: Set<ToolbarAction>

    @Inject
    internal lateinit var activityContributions: Set<AppActivityContribution>

    @Inject
    internal lateinit var deepLinks: DeepLinkResolver

    @Inject
    internal lateinit var logger: AppLogger

    @Inject
    internal lateinit var access: AppAccessSession

    override val navigator: Navigator get() = router

    override fun onCreate(savedInstanceState: Bundle?) {
        (application as HomeApplication).graph.inject(this)
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
        binding.glassBar.bind(binding.fragmentContainer, glassChrome)
        binding.appbar.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> glassChrome.updateBar(binding.appbar.height) }
        router = FragmentRouter(
            fragmentManager = supportFragmentManager,
            containerId = R.id.fragment_container,
            registry = features,
            logger = logger,
            savedState = FragmentRouter.RouterSavedState.from(savedInstanceState),
        )
        lifecycle.addObserver(router)
        actionsMenu = ToolbarChrome(binding, router, toolbarActions) { onBackPressedDispatcher.onBackPressed() }
        tabs = BottomTabs(binding.homeNoticeAnchor, features, router)
        binding.applySystemBarInsets(tabs)
        attachContributions()
        onBackPressedDispatcher.addCallback(this) { if (!router.back()) finish() }
        deepLinks.open(intent, router)
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                combine(router.chrome, access.state, ::render).collect { }
            }
        }
    }

    private fun attachContributions() {
        val slots = ToolbarSlots(binding.toolbarTitleRow, binding.toolbarEndSlot, binding.appbarBelowSlot)
        toolbarContributions.sortedBy { it.order }.forEach { it.attach(this, slots, router) }
        activityContributions.sortedBy { it.order }.forEach { it.attach(this) }
    }

    private fun render(chrome: AppChromeState, accessState: AppAccessState) {
        val visibleTabs = features.tabs().filter { accessState.allows(it.module) }
        val withoutSections = accessState is AppAccessState.Ready && visibleTabs.isEmpty()
        binding.fragmentContainer.isVisible = !withoutSections
        binding.homeNoAccess.isVisible = withoutSections
        actionsMenu.render(chrome, accessState)
        tabs.render(chrome, accessState)
        if (withoutSections) return
        if (visibleTabs.none { it.id == chrome.tabId }) router.selectTab(visibleTabs.first().id)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        deepLinks.open(intent, router)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        router.onHostSaveState(outState)
        super.onSaveInstanceState(outState)
    }

    override fun onDestroy() {
        if (::actionsMenu.isInitialized) actionsMenu.dismiss()
        super.onDestroy()
    }
}

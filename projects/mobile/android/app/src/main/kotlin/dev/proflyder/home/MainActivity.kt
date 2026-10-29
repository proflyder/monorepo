package dev.proflyder.home

import android.os.Bundle
import android.view.Menu
import androidx.activity.OnBackPressedCallback
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
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.databinding.ActivityMainBinding
import kotlinx.coroutines.launch

class MainActivity : AppCompatActivity(), NavigatorHost, ViewModelFactoryProvider {
    private lateinit var router: FragmentRouter
    private lateinit var binding: ActivityMainBinding
    private lateinit var features: FeatureRegistry

    private val bottomNavigation: BottomNavigationView get() = binding.homeNoticeAnchor
    private val toolbar: MaterialToolbar get() = binding.toolbar

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

        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    if (!router.back()) finish()
                }
            },
        )

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    router.currentTabId.collect { featureId ->
                        val itemId = itemIdByFeature[featureId] ?: return@collect
                        if (bottomNavigation.selectedItemId != itemId) {
                            syncingSelection = true
                            bottomNavigation.selectedItemId = itemId
                            syncingSelection = false
                        }
                    }
                }
                launch {
                    router.currentTitleRes.collect { toolbar.setTitle(it) }
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

    override fun onSaveInstanceState(outState: Bundle) {
        router.onHostSaveState(outState)
        super.onSaveInstanceState(outState)
    }

    override fun viewModelFactory(viewModelClass: Class<out ViewModel>): ViewModelProvider.Factory =
        (application as HomeApplication).graph.viewModelFactories.viewModelFactory(viewModelClass)

    private fun setupBottomNavigation() {
        val menu: Menu = bottomNavigation.menu
        menu.clear()
        var itemId = 1
        features.all().forEach { entry: FeatureEntry ->
            menu.add(Menu.NONE, itemId, entry.order, entry.titleRes).setIcon(entry.iconRes)
            itemIdByFeature[entry.id] = itemId
            featureByItemId[itemId] = entry.id
            itemId++
        }
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

    private fun setupToolbar() {
        toolbar.setNavigationOnClickListener { onBackPressedDispatcher.onBackPressed() }
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
}

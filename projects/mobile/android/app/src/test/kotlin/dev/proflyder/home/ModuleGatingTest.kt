package dev.proflyder.home

import android.content.Context
import android.view.LayoutInflater
import androidx.appcompat.view.ContextThemeWrapper
import androidx.fragment.app.Fragment
import com.google.android.material.bottomnavigation.BottomNavigationView
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.AppChromeState
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.databinding.ActivityMainBinding
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ModuleGatingTest {
    private class FakeNavigator(initial: String) : Navigator {
        private val chromeState = MutableStateFlow(AppChromeState(initial, 0))
        override val chrome: StateFlow<AppChromeState> = chromeState
        override fun selectTab(featureId: String) = Unit
        override fun navigate(destination: Destination) = Unit
        override fun openRoute(route: String): Boolean = false
        override fun back(): Boolean = false
    }

    private class FakeTab(
        override val id: String,
        override val order: Int,
        override val module: AppModule,
    ) : FeatureEntry {
        override val titleRes: Int = android.R.string.ok
        override val iconRes: Int = android.R.drawable.ic_menu_help
        override val rootDestination: Destination = object : Destination {
            override val featureId: String = id
            override val route: String = id
        }

        override fun createFragment(destination: Destination): Fragment = Fragment()
    }

    private class FakeAction(
        override val itemId: Int,
        override val module: AppModule,
    ) : dev.proflyder.home.core.navigation.ToolbarAction {
        override val order: Int = itemId
        override val titleRes: Int = android.R.string.ok
        override val iconRes: Int = android.R.drawable.ic_menu_help

        override fun perform(navigator: Navigator) = Unit
    }

    private val restricted = AppAccessState.Ready(
        AppAccess("Phone", setOf(AppModule.BASE, AppModule.HOME), owner = false),
    )

    private val everything = AppAccessState.Ready(
        AppAccess("Owner", AppModule.entries.toSet(), owner = true),
    )

    private val entries = listOf(
        FakeTab("home", 0, AppModule.HOME),
        FakeTab("chat", 1, AppModule.CHAT),
        FakeTab("system", 2, AppModule.SYSTEM),
    )

    private fun context(): Context = ContextThemeWrapper(
        org.robolectric.RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_DayNight,
    )

    @Test
    fun bottomTabsOnlyShowModulesTheTokenAllows() {
        val view = BottomNavigationView(context())
        val tabs = BottomTabs(view, FeatureRegistry(entries), FakeNavigator("home"))

        tabs.render(AppChromeState("home", 0), everything)
        assertEquals(listOf("home", "chat", "system"), tabs.visibleTabIds)
        assertEquals(3, view.menu.size())

        tabs.render(AppChromeState("chat", 0), restricted)
        assertEquals(listOf("home"), tabs.visibleTabIds)
        assertEquals(1, view.menu.size())
    }

    @Test
    fun toolbarActionsOnlyShowModulesTheTokenAllowsAndOverlayKeepsThemHidden() {
        val binding = ActivityMainBinding.inflate(LayoutInflater.from(context()))
        val chrome = AppChromeState("home", R.string.app_actions)
        val actions = setOf(FakeAction(1001, AppModule.BASE), FakeAction(1002, AppModule.SYSTEM))
        val toolbar = ToolbarChrome(binding, FakeNavigator("home"), actions) {}

        toolbar.render(chrome, restricted)
        assertEquals(listOf(1001), toolbar.visibleActions.map { it.itemId })
        assertTrue(binding.toolbar.menu.findItem(R.id.action_menu).isVisible)

        toolbar.render(chrome.copy(overlay = true), restricted)
        assertFalse(binding.toolbar.menu.findItem(R.id.action_menu).isVisible)

        toolbar.render(chrome.copy(overlay = true), everything)
        assertEquals(listOf(1001, 1002), toolbar.visibleActions.map { it.itemId })
        assertFalse(binding.toolbar.menu.findItem(R.id.action_menu).isVisible)
    }
}

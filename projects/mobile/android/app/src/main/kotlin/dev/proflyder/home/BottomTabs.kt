package dev.proflyder.home

import android.view.Menu
import android.view.MenuItem
import androidx.core.view.ViewCompat
import androidx.core.view.isVisible
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.google.android.material.navigation.NavigationBarView
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.navigation.AppChromeState
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.Navigator

internal class BottomTabs(
    private val view: BottomNavigationView,
    private val features: FeatureRegistry,
    private val navigator: Navigator,
) {
    private var tabs: List<FeatureEntry> = features.tabs()
    private var tabIds: List<String> = tabs.map(FeatureEntry::id)
    private val onSelected = NavigationBarView.OnItemSelectedListener(::onItemSelected)
    private var keyboardShown = false
    private var hidden = navigator.chrome.value.hidesBottomBar

    val coversContent: Boolean
        get() = !keyboardShown && !navigator.chrome.value.hidesBottomBar && tabs.isNotEmpty()

    init {
        HomePopupMenu.markObstacle(view)
        renderMenu()
        render(navigator.chrome.value, AppAccessState.Unknown)
    }

    fun render(chrome: AppChromeState, access: AppAccessState) {
        applyAccess(access)
        val index = tabIds.indexOf(chrome.tabId)
        view.setOnItemSelectedListener(null)
        if (index >= 0 && view.selectedItemId != itemIdOf(index)) view.selectedItemId = itemIdOf(index)
        view.setOnItemSelectedListener(onSelected)
        renderVisibility()
        if (hidden == chrome.hidesBottomBar) return
        hidden = chrome.hidesBottomBar
        ViewCompat.requestApplyInsets(view.rootView)
    }

    fun keyboard(shown: Boolean) {
        keyboardShown = shown
        renderVisibility()
    }

    private fun applyAccess(access: AppAccessState) {
        val next = features.tabs().filter { access.allows(it.module) }
        if (next.map(FeatureEntry::id) == tabIds) return
        tabs = next
        tabIds = next.map(FeatureEntry::id)
        renderMenu()
        renderVisibility()
    }

    private fun renderMenu() {
        view.menu.clear()
        tabs.forEachIndexed { index, entry ->
            view.menu.add(Menu.NONE, itemIdOf(index), entry.order, entry.titleRes).setIcon(entry.iconRes)
        }
    }

    private fun renderVisibility() {
        view.isVisible = coversContent
    }

    private fun onItemSelected(item: MenuItem): Boolean {
        tabIds.getOrNull(item.itemId - FIRST_ITEM_ID)?.let(navigator::selectTab)
        return false
    }

    private fun itemIdOf(index: Int): Int = index + FIRST_ITEM_ID

    private companion object {
        const val FIRST_ITEM_ID = 1
    }
}

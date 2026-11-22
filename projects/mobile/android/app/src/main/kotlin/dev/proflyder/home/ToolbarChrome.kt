package dev.proflyder.home

import android.view.Gravity
import android.view.Menu
import android.view.MenuItem
import android.view.View
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.navigation.AppChromeState
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarAction
import dev.proflyder.home.databinding.ActivityMainBinding

internal class ToolbarChrome(
    private val binding: ActivityMainBinding,
    private val navigator: Navigator,
    actions: Set<ToolbarAction>,
    onBack: () -> Unit,
) {
    private val actions = actions.sortedBy { it.order }
    private var popup: HomePopupMenu? = null
    private var overlay: Boolean? = null

    internal val contentView: View?
        get() = popup?.contentView

    init {
        with(binding.toolbar) {
            title = null
            inflateMenu(R.menu.menu_main)
            menu.findItem(R.id.action_menu)?.contentDescription = context.getString(R.string.app_actions)
            setNavigationOnClickListener { onBack() }
            setOnMenuItemClickListener(::onMenuItem)
        }
    }

    fun render(chrome: AppChromeState) {
        binding.toolbarTitle.setText(chrome.titleRes)
        renderBack(chrome.canGoBack)
        if (overlay == chrome.overlay) return
        overlay = chrome.overlay
        binding.toolbar.menu.findItem(R.id.action_menu)?.isVisible = !chrome.overlay
        dismiss()
    }

    fun dismiss() {
        popup?.dismiss()
    }

    private fun renderBack(canGoBack: Boolean) {
        val toolbar = binding.toolbar
        if (!canGoBack) {
            toolbar.navigationIcon = null
            return
        }
        toolbar.setNavigationIcon(R.drawable.app_ic_back)
        toolbar.setNavigationContentDescription(R.string.app_back)
    }

    private fun onMenuItem(item: MenuItem): Boolean {
        if (item.itemId != R.id.action_menu) return false
        binding.toolbar.findViewById<View>(R.id.action_menu)?.let(::toggle)
        return true
    }

    private fun toggle(anchor: View) {
        if (popup?.isShowing == true) {
            dismiss()
            return
        }
        popup = HomePopupMenu(anchor, ::populate, ::perform).also { it.show(Gravity.END) }
    }

    private fun populate(menu: Menu) {
        actions.forEach { menu.add(Menu.NONE, it.itemId, it.order, it.titleRes).setIcon(it.iconRes) }
    }

    private fun perform(itemId: Int) {
        actions.firstOrNull { it.itemId == itemId }?.perform(navigator)
    }
}

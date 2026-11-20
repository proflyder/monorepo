package dev.proflyder.home

import android.view.Gravity
import android.view.View
import dev.proflyder.home.core.ui.HomePopupMenu

internal class ToolbarActionsMenu(
    private val onAction: (Int) -> Unit,
) {
    private var popup: HomePopupMenu? = null

    internal val contentView: View?
        get() = popup?.contentView

    fun toggle(anchor: View) {
        when (popup?.isShowing) {
            true -> popup?.dismiss()
            else -> show(anchor)
        }
    }

    fun dismiss() {
        popup?.dismiss()
    }

    private fun show(anchor: View) {
        popup = HomePopupMenu(anchor, R.menu.menu_actions, onAction).also { it.show(Gravity.END) }
    }
}

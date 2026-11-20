package dev.proflyder.home.core.designsystem

import android.graphics.Color
import android.view.Gravity
import android.view.LayoutInflater
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.PopupWindow
import android.widget.TextView
import androidx.annotation.MenuRes
import androidx.appcompat.widget.PopupMenu
import androidx.core.graphics.drawable.toDrawable
import androidx.core.view.isVisible
import androidx.core.view.size

class HomePopupMenu(
    private val anchor: View,
    @MenuRes menuRes: Int,
    private val onItem: (Int) -> Unit,
) {
    private val source = PopupMenu(anchor.context, anchor).apply { inflate(menuRes) }
    private var window: PopupWindow? = null

    val menu: Menu
        get() = source.menu

    val contentView: View?
        get() = window?.contentView?.takeIf { window?.isShowing == true }

    val isShowing: Boolean
        get() = window?.isShowing == true

    fun show(gravity: Int = Gravity.START) {
        window = build(content()).also { it.showAsDropDown(anchor, 0, 0, gravity) }
    }

    fun toggle(gravity: Int = Gravity.START) {
        when {
            isShowing -> dismiss()
            else -> show(gravity)
        }
    }

    fun dismiss() {
        window?.dismiss()
    }

    private fun content(): View {
        val inflater = LayoutInflater.from(anchor.context)
        val list = inflater.inflate(R.layout.ds_popup_menu, FrameLayout(anchor.context), false) as LinearLayout
        visibleItems().forEach { list.addView(row(inflater, list, it)) }
        return list
    }

    private fun visibleItems(): List<MenuItem> = (0 until menu.size).map(menu::getItem).filter { it.isVisible }

    private fun row(inflater: LayoutInflater, parent: ViewGroup, item: MenuItem): View {
        val row = inflater.inflate(R.layout.ds_popup_menu_item, parent, false)
        row.id = item.itemId
        val icon = row.findViewById<ImageView>(R.id.ds_popup_menu_icon)
        icon.setImageDrawable(item.icon)
        icon.isVisible = item.icon != null
        row.findViewById<TextView>(R.id.ds_popup_menu_label).text = item.title
        row.setOnClickListener { select(item.itemId) }
        return row
    }

    private fun select(itemId: Int) {
        dismiss()
        onItem(itemId)
    }

    private fun build(content: View): PopupWindow {
        val popup = PopupWindow(content, ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, true)
        popup.animationStyle = R.style.Animation_Home_PopupMenu
        popup.elevation = anchor.resources.getDimension(R.dimen.ds_popup_elevation)
        popup.setBackgroundDrawable(Color.TRANSPARENT.toDrawable())
        popup.isOutsideTouchable = true
        return popup
    }
}

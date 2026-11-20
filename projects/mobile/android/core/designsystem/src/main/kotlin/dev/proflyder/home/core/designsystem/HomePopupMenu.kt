package dev.proflyder.home.core.designsystem

import android.graphics.Color
import android.graphics.Rect
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
import android.widget.ScrollView
import android.widget.TextView
import androidx.annotation.MenuRes
import androidx.appcompat.widget.PopupMenu
import androidx.core.graphics.Insets
import androidx.core.graphics.drawable.toDrawable
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
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
        get() = (window?.contentView as? ViewGroup)?.getChildAt(0)?.takeIf { window?.isShowing == true }

    val isShowing: Boolean
        get() = window?.isShowing == true

    var placement: PopupPlacement? = null
        private set

    fun show(gravity: Int = Gravity.START) {
        val list = content()
        list.measure(UNBOUNDED, UNBOUNDED)
        val bounds = visibleBounds()
        val anchorBox = windowBox(anchor)
        val place = PopupPlacements.place(anchorBox.top, anchorBox.bottom, bounds.top, bounds.bottom, list.measuredHeight, ANCHOR_GAP)
        val x = PopupPlacements.clampX(startX(gravity, anchorBox, list.measuredWidth), list.measuredWidth, bounds.left, bounds.right)
        placement = place
        window = build(scrolling(list), list.measuredWidth, place.height).also {
            it.showAtLocation(anchor, Gravity.TOP or Gravity.START, x, place.top)
        }
    }

    private fun startX(gravity: Int, anchorBox: Rect, width: Int): Int = when (gravity) {
        Gravity.END -> anchorBox.right - width
        else -> anchorBox.left
    }

    private fun scrolling(list: View): View = ScrollView(anchor.context).apply {
        isVerticalScrollBarEnabled = true
        addView(list)
    }

    private fun visibleBounds(): Rect {
        val root = anchor.rootView
        val types = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime() or WindowInsetsCompat.Type.displayCutout()
        val insets = ViewCompat.getRootWindowInsets(anchor)?.getInsets(types) ?: Insets.NONE
        val bounds = Rect(insets.left, insets.top, root.width - insets.right, root.height - insets.bottom)
        obstacles(root).map(::windowBox).forEach { exclude(bounds, it, windowBox(anchor)) }
        return bounds
    }

    private fun exclude(bounds: Rect, obstacle: Rect, anchorBox: Rect) {
        when {
            obstacle.top >= anchorBox.bottom -> bounds.bottom = minOf(bounds.bottom, obstacle.top)
            obstacle.bottom <= anchorBox.top -> bounds.top = maxOf(bounds.top, obstacle.bottom)
        }
    }

    private fun obstacles(view: View): List<View> = when {
        !view.isShown -> emptyList()
        view.getTag(R.id.ds_popup_obstacle) == true -> listOf(view)
        view is ViewGroup -> (0 until view.childCount).flatMap { obstacles(view.getChildAt(it)) }
        else -> emptyList()
    }

    private fun windowBox(view: View): Rect {
        val location = IntArray(2).also(view::getLocationInWindow)
        return Rect(location[0], location[1], location[0] + view.width, location[1] + view.height)
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

    private fun build(content: View, width: Int, height: Int): PopupWindow {
        val popup = PopupWindow(content, width, height, true)
        popup.animationStyle = R.style.Animation_Home_PopupMenu
        popup.elevation = anchor.resources.getDimension(R.dimen.ds_popup_elevation)
        popup.setBackgroundDrawable(Color.TRANSPARENT.toDrawable())
        popup.isOutsideTouchable = true
        return popup
    }

    companion object {
        const val ANCHOR_GAP = 0
        private val UNBOUNDED = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)

        fun markObstacle(view: View) = view.setTag(R.id.ds_popup_obstacle, true)
    }
}

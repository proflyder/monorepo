package dev.proflyder.home.core.ui

import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import androidx.core.view.children
import androidx.core.view.isVisible
import androidx.core.widget.NestedScrollView
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.designsystem.glass.GlassHeaderLayout
import dev.proflyder.home.core.designsystem.glass.GlassScreenHeader
import androidx.core.graphics.Insets
import androidx.core.view.OnApplyWindowInsetsListener
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.glass.StickyGlass
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.designsystem.list.StickyPinEdge

data class ContentInsets(val extraBottom: Int = 0, val bottom: Boolean = true)

internal class ContentInsetsListener(view: View, private val spec: ContentInsets) : OnApplyWindowInsetsListener {
    private val initial = InitialPadding(view.paddingLeft, view.paddingTop, view.paddingRight, view.paddingBottom)
    private var top = 0

    val pinEdge = StickyPinEdge { top }

    fun bindSticky(list: RecyclerView) {
        val manager = list.layoutManager as? StickyHeadersLayoutManager ?: return
        manager.pinEdge = pinEdge
        StickyGlass.attach(list)
    }

    override fun onApplyWindowInsets(view: View, insets: WindowInsetsCompat): WindowInsetsCompat {
        val atTop = !view.canScrollVertically(-1)
        val changed = top != insets.topInset()
        top = insets.topInset()
        view.updatePadding(top = initial.top + top, bottom = bottomPadding(insets))
        if (changed && atTop) (view as? RecyclerView)?.scrollToPosition(0)
        return consumed(insets)
    }

    private fun consumed(insets: WindowInsetsCompat): WindowInsetsCompat {
        val builder = WindowInsetsCompat.Builder(insets)
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.NONE)
            .setInsets(WindowInsetsCompat.Type.displayCutout(), Insets.NONE)
        if (!spec.bottom) return builder.build()
        return builder
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.NONE)
            .setInsets(WindowInsetsCompat.Type.ime(), Insets.NONE)
            .build()
    }

    private fun bottomPadding(insets: WindowInsetsCompat): Int = when {
        spec.bottom -> initial.bottom + insets.bottomObstruction() + spec.extraBottom
        else -> initial.bottom
    }
}

fun View.contentInsetTarget(): View? {
    val scroller = primaryScroller()
    return when {
        scroller != null && !scroller.hasFixedContentAbove(this) -> scroller
        containsHeaderLayout() -> null
        else -> this
    }
}

private fun View.containsHeaderLayout(): Boolean = when (this) {
    is GlassHeaderLayout -> true
    is ViewGroup -> children.any { it.containsHeaderLayout() }
    else -> false
}

private fun View.hasFixedContentAbove(root: View): Boolean {
    var child = this
    while (child !== root) {
        val parent = child.parent as? ViewGroup ?: return false
        if (parent.stacksVertically() && parent.children.takeWhile { it !== child }.any { it.isVisible }) return true
        child = parent
    }
    return false
}

private fun ViewGroup.stacksVertically(): Boolean = this is LinearLayout && orientation == LinearLayout.VERTICAL

private fun View.primaryScroller(): View? = when (this) {
    is GlassScreenHeader -> null
    is RecyclerView -> takeIf { layoutManager?.canScrollVertically() == true }
    is NestedScrollView, is ScrollView -> this
    is ViewGroup -> children.firstNotNullOfOrNull { it.primaryScroller() }
    else -> null
}

fun Fragment.fabContentInsets(): ContentInsets = ContentInsets(extraBottom = resources.getDimensionPixelSize(R.dimen.ui_fab_clearance))

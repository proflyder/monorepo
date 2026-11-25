package dev.proflyder.home.core.designsystem.glass

import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import androidx.core.view.updatePadding
import androidx.core.widget.NestedScrollView
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager

fun View.applyGlassChrome() {
    val binding = GlassContentBinding(this)
    addOnAttachStateChangeListener(binding)
    if (isAttachedToWindow) binding.onViewAttachedToWindow(this)
}

private class GlassContentBinding(private val root: View) : View.OnAttachStateChangeListener {
    private var bound: BoundContent? = null

    override fun onViewAttachedToWindow(view: View) {
        val chrome = view.context.glassChrome() ?: return
        if (view.parentScrolls()) return
        val target = bound?.target ?: view.insetTarget() ?: return
        bound = BoundContent(target, chrome, bound?.baseTop ?: target.paddingTop).also(BoundContent::start)
    }

    override fun onViewDetachedFromWindow(view: View) {
        bound?.stop()
    }

    private fun View.insetTarget(): View? {
        val scroller = primaryScroller()
        (scroller as? ViewGroup)?.clipToPadding = false
        return when {
            scroller != null && !scroller.hasFixedContentAbove(root) -> scroller
            containsScreenHeader() -> null
            else -> this
        }
    }
}

private class BoundContent(
    val target: View,
    private val chrome: GlassChrome,
    val baseTop: Int,
) : GlassChromeListener, View.OnLayoutChangeListener {
    private val list = target as? RecyclerView
    private val scrolls = ContentScrolls(chrome)
    private val sticky = (list?.layoutManager as? StickyHeadersLayoutManager)?.let { StickyGlass(list, it, chrome) }

    fun start() {
        chrome.addListener(this)
        target.addOnLayoutChangeListener(this)
        list?.addOnScrollListener(scrolls)
        sticky?.start()
    }

    fun stop() {
        chrome.removeListener(this)
        target.removeOnLayoutChangeListener(this)
        list?.removeOnScrollListener(scrolls)
        sticky?.stop()
    }

    override fun onGlassChromeChanged(chrome: GlassChrome) = applyInset()

    override fun onLayoutChange(view: View, l: Int, t: Int, r: Int, b: Int, ol: Int, ot: Int, or: Int, ob: Int) = applyInset()

    private fun applyInset() {
        val top = baseTop + (chrome.contentTop - target.windowTop()).coerceAtLeast(0)
        if (target.paddingTop == top) return
        val atTop = !target.canScrollVertically(-1)
        target.updatePadding(top = top)
        if (atTop) list?.scrollToPosition(0)
    }
}

private class ContentScrolls(private val chrome: GlassChrome) : RecyclerView.OnScrollListener() {
    override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
        chrome.contentScrolled(dy, atTop = !recyclerView.canScrollVertically(-1))
    }
}

private fun View.parentScrolls(): Boolean {
    var current = parent as? View
    while (current != null) {
        if (current is GlassScreenHeader || current.scrollsVertically()) return true
        current = current.parent as? View
    }
    return false
}

private fun View.scrollsVertically(): Boolean = when (this) {
    is RecyclerView -> layoutManager?.canScrollVertically() == true
    is NestedScrollView, is ScrollView -> true
    else -> false
}

private fun View.containsScreenHeader(): Boolean = when (this) {
    is GlassScreenHeader -> true
    is ViewGroup -> children().any { it.containsScreenHeader() }
    else -> false
}

private fun View.hasFixedContentAbove(root: View): Boolean {
    var child = this
    while (child !== root) {
        val parent = child.parent as? ViewGroup ?: return false
        if (parent.stacksVertically() && parent.children().takeWhile { it !== child }.any { it.visibility != View.GONE }) return true
        child = parent
    }
    return false
}

private fun ViewGroup.stacksVertically(): Boolean = this is LinearLayout && orientation == LinearLayout.VERTICAL

private fun ViewGroup.children(): List<View> = (0 until childCount).map(::getChildAt)

internal fun View.primaryScroller(): View? = when {
    this is GlassScreenHeader -> null
    this is RecyclerView -> takeIf { scrollsVertically() }
    this is NestedScrollView || this is ScrollView -> this
    this is ViewGroup -> children().firstNotNullOfOrNull { it.primaryScroller() }
    else -> null
}

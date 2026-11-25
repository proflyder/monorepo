package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Canvas
import android.graphics.Rect
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import androidx.core.view.updatePadding
import androidx.core.widget.NestedScrollView
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.AppBarLayout

fun interface AppBarHeightListener {
    fun onAppBarHeightChanged(height: Int)
}

interface AppBarHost {
    val appBarHeight: Int

    fun addAppBarHeightListener(listener: AppBarHeightListener)

    fun removeAppBarHeightListener(listener: AppBarHeightListener)

    fun pinUnderBar(header: PinnedBarHeader?)

    fun backdropChanged()
}

data class PinnedBarHeader(val view: View, val left: Int, val offset: Int) {
    val visibleHeight: Int get() = (view.height + offset).coerceAtLeast(0)
}

class AppBarHeightTracker : AppBarHost {
    private val listeners = mutableListOf<AppBarHeightListener>()
    var pinnedHeaderListener: ((PinnedBarHeader?) -> Unit)? = null
    var backdropListener: (() -> Unit)? = null
    private var pinned: PinnedBarHeader? = null
    override var appBarHeight: Int = 0
        private set

    override fun addAppBarHeightListener(listener: AppBarHeightListener) {
        listeners.add(listener)
        listener.onAppBarHeightChanged(appBarHeight)
    }

    override fun removeAppBarHeightListener(listener: AppBarHeightListener) {
        listeners.remove(listener)
    }

    override fun pinUnderBar(header: PinnedBarHeader?) {
        if (header == pinned) return
        pinned = header
        pinnedHeaderListener?.invoke(header)
    }

    override fun backdropChanged() {
        backdropListener?.invoke()
    }

    fun update(height: Int) {
        if (height == appBarHeight) return
        appBarHeight = height
        listeners.toList().forEach { it.onAppBarHeightChanged(height) }
    }
}

fun Context.appBarHost(): AppBarHost? {
    var current: Context? = this
    while (current is ContextWrapper) {
        if (current is AppBarHost) return current
        current = current.baseContext
    }
    return current as? AppBarHost
}

fun View.applyAppBarContentPadding() {
    val source = context.appBarHost() ?: return
    val content = appBarInsetTarget()
    val baseTop = content.paddingTop
    val listener = AppBarHeightListener { height -> content.updatePadding(top = baseTop + height) }
    listener.onAppBarHeightChanged(source.appBarHeight)
    val binding = AttachedHeightBinding(source, listener)
    content.addOnAttachStateChangeListener(binding)
    if (content.isAttachedToWindow) binding.onViewAttachedToWindow(content)
}

private class AttachedHeightBinding(
    private val source: AppBarHost,
    private val listener: AppBarHeightListener,
) : View.OnAttachStateChangeListener {
    override fun onViewAttachedToWindow(view: View) = source.addAppBarHeightListener(listener)

    override fun onViewDetachedFromWindow(view: View) = source.removeAppBarHeightListener(listener)
}

internal fun View.appBarInsetTarget(): View {
    val scroller = primaryScroller()
    (scroller as? ViewGroup)?.clipToPadding = false
    innerAppBar()?.let { return it.preparedAsInsetTarget() }
    return scroller?.takeUnless { it.hasFixedContentAbove(this) } ?: this
}

private fun AppBarLayout.preparedAsInsetTarget(): View {
    (parent as? ViewGroup)?.clipChildren = false
    addOnOffsetChangedListener(HideUnderBar)
    return this
}

internal object HideUnderBar : AppBarLayout.OnOffsetChangedListener {
    override fun onOffsetChanged(appBarLayout: AppBarLayout, verticalOffset: Int) {
        val edge = appBarLayout.paddingTop - verticalOffset
        appBarLayout.clipBounds = Rect(0, edge, appBarLayout.width, appBarLayout.height)
    }
}

fun View.trackGlassBackdrop() {
    val source = context.appBarHost() ?: return
    (primaryScroller() as? RecyclerView)?.addItemDecoration(BackdropNotifier(source))
}

private class BackdropNotifier(private val source: AppBarHost) : RecyclerView.ItemDecoration() {
    override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        source.backdropChanged()
    }
}

private fun View.innerAppBar(): AppBarLayout? = when (this) {
    is AppBarLayout -> this
    is RecyclerView -> null
    is ViewGroup -> children().firstNotNullOfOrNull { it.innerAppBar() }
    else -> null
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
    this is AppBarLayout -> null
    this is RecyclerView -> verticalContent()
    this is NestedScrollView -> this
    this is ScrollView -> this
    this is ViewGroup -> children().firstNotNullOfOrNull { it.primaryScroller() }
    else -> null
}

private fun RecyclerView.verticalContent(): View? {
    val manager = layoutManager ?: return this
    return when {
        manager.canScrollVertically() -> this
        else -> null
    }
}

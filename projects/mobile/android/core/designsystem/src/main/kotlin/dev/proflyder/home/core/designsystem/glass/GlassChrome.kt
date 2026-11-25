package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import android.view.View
import android.view.ViewGroup
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

    fun mergeBarEdge(merged: Boolean)
}

class AppBarHeightTracker : AppBarHost {
    private val listeners = mutableListOf<AppBarHeightListener>()
    var edgeMergeListener: ((Boolean) -> Unit)? = null
    private var edgeMerged = false
    override var appBarHeight: Int = 0
        private set

    override fun addAppBarHeightListener(listener: AppBarHeightListener) {
        listeners.add(listener)
        listener.onAppBarHeightChanged(appBarHeight)
    }

    override fun removeAppBarHeightListener(listener: AppBarHeightListener) {
        listeners.remove(listener)
    }

    override fun mergeBarEdge(merged: Boolean) {
        if (merged == edgeMerged) return
        edgeMerged = merged
        edgeMergeListener?.invoke(merged)
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
    source.addAppBarHeightListener(listener)
    content.addOnAttachStateChangeListener(DetachUnregister(source, listener))
}

private class DetachUnregister(
    private val source: AppBarHost,
    private val listener: AppBarHeightListener,
) : View.OnAttachStateChangeListener {
    override fun onViewAttachedToWindow(view: View) = Unit

    override fun onViewDetachedFromWindow(view: View) {
        source.removeAppBarHeightListener(listener)
        view.removeOnAttachStateChangeListener(this)
    }
}

internal fun View.appBarInsetTarget(): View {
    val scroller = primaryScroller() ?: return this
    val parent = scroller.parent as? ViewGroup
    val header = parent?.let { group -> (0 until group.childCount).map(group::getChildAt).firstOrNull { it is AppBarLayout } }
    val target = header ?: scroller
    (scroller as? ViewGroup)?.clipToPadding = false
    parent?.takeIf { header != null }?.clipChildren = false
    return target
}

internal fun View.primaryScroller(): View? = when {
    this is AppBarLayout -> null
    this is RecyclerView -> verticalContent()
    this is NestedScrollView -> this
    this is ScrollView -> this
    this is ViewGroup -> (0 until childCount).firstNotNullOfOrNull { getChildAt(it).primaryScroller() }
    else -> null
}

private fun RecyclerView.verticalContent(): View? {
    val manager = layoutManager ?: return this
    return when {
        manager.canScrollVertically() -> this
        else -> null
    }
}

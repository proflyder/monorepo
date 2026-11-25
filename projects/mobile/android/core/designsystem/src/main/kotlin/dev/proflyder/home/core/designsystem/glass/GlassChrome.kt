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

interface AppBarHeightSource {
    val appBarHeight: Int

    fun addAppBarHeightListener(listener: AppBarHeightListener)

    fun removeAppBarHeightListener(listener: AppBarHeightListener)
}

class AppBarHeightTracker : AppBarHeightSource {
    private val listeners = mutableListOf<AppBarHeightListener>()
    override var appBarHeight: Int = 0
        private set

    override fun addAppBarHeightListener(listener: AppBarHeightListener) {
        listeners.add(listener)
        listener.onAppBarHeightChanged(appBarHeight)
    }

    override fun removeAppBarHeightListener(listener: AppBarHeightListener) {
        listeners.remove(listener)
    }

    fun update(height: Int) {
        if (height == appBarHeight) return
        appBarHeight = height
        listeners.toList().forEach { it.onAppBarHeightChanged(height) }
    }
}

fun Context.appBarHeightSource(): AppBarHeightSource? {
    var current: Context? = this
    while (current is ContextWrapper) {
        if (current is AppBarHeightSource) return current
        current = current.baseContext
    }
    return current as? AppBarHeightSource
}

fun View.applyAppBarContentPadding() {
    val source = context.appBarHeightSource() ?: return
    val content = primaryScroller() ?: this
    (content as? ViewGroup)?.clipToPadding = content === this
    val baseTop = content.paddingTop
    val listener = AppBarHeightListener { height -> content.updatePadding(top = baseTop + height) }
    source.addAppBarHeightListener(listener)
    content.addOnAttachStateChangeListener(DetachUnregister(source, listener))
}

private class DetachUnregister(
    private val source: AppBarHeightSource,
    private val listener: AppBarHeightListener,
) : View.OnAttachStateChangeListener {
    override fun onViewAttachedToWindow(view: View) = Unit

    override fun onViewDetachedFromWindow(view: View) {
        source.removeAppBarHeightListener(listener)
        view.removeOnAttachStateChangeListener(this)
    }
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

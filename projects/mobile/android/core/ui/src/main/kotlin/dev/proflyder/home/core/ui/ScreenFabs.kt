package dev.proflyder.home.core.ui

import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.core.view.children
import androidx.core.view.isVisible
import dev.proflyder.home.core.designsystem.HomeMotion
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import com.google.android.material.floatingactionbutton.FloatingActionButton

fun View.liftScreenFabs() {
    screenFabs().forEach(View::liftAboveBottomInsets)
}

fun View.screenFabs(): Sequence<View> = when {
    isScreenFab() -> sequenceOf(this)
    this is ViewGroup -> children.flatMap { it.screenFabs() }
    else -> emptySequence()
}

private fun View.isScreenFab(): Boolean {
    val fab = this is FloatingActionButton || this is ExtendedFloatingActionButton
    val gravity = (layoutParams as? FrameLayout.LayoutParams)?.gravity ?: return false
    return fab && gravity != FrameLayout.LayoutParams.UNSPECIFIED_GRAVITY &&
        gravity and Gravity.VERTICAL_GRAVITY_MASK == Gravity.BOTTOM
}

fun View.setShown(shown: Boolean) {
    val target = if (shown) 1f else 0f
    val settled = isVisible == shown && alpha == target
    if (settled || animatingTo == target) return
    animatingTo = target
    if (shown && !isVisible) collapseForEntry()
    if (shown) isVisible = true
    animate().cancel()
    animate().scaleX(target).scaleY(target).alpha(target)
        .setDuration(HomeMotion.selectionDuration(context).toLong())
        .setInterpolator(if (shown) HomeMotion.enterInterpolator(context) else HomeMotion.exitInterpolator(context))
        .withEndAction { settle(shown) }
        .start()
}

private var View.animatingTo: Float?
    get() = getTag(R.id.ui_fab_target) as? Float
    set(value) = setTag(R.id.ui_fab_target, value)

private fun View.collapseForEntry() {
    alpha = 0f
    scaleX = 0f
    scaleY = 0f
}

private fun View.settle(shown: Boolean) {
    animatingTo = null
    visibility = if (shown) View.VISIBLE else View.INVISIBLE
}

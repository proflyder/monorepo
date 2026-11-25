package dev.proflyder.home.core.ui

import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.core.view.children
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

fun FloatingActionButton.setShown(shown: Boolean) {
    if (shown) show() else hide()
}

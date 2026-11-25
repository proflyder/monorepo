package dev.proflyder.home.core.ui

import android.graphics.Rect
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsAnimationCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsCompat.Type
import androidx.core.view.doOnAttach
import androidx.core.view.doOnDetach
import androidx.core.view.marginBottom
import androidx.core.view.updateLayoutParams
import androidx.core.view.updatePadding
import androidx.core.widget.NestedScrollView
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.textfield.TextInputLayout

data class InitialPadding(val left: Int, val top: Int, val right: Int, val bottom: Int)

fun View.doOnApplyInsets(block: (View, WindowInsetsCompat, InitialPadding) -> Unit) {
    val initial = InitialPadding(paddingLeft, paddingTop, paddingRight, paddingBottom)
    ViewCompat.setOnApplyWindowInsetsListener(this) { view, insets ->
        block(view, insets, initial)
        insets
    }
    requestInsetsWhenAttached()
}

fun View.requestInsetsWhenAttached() {
    when {
        isAttachedToWindow -> ViewCompat.requestApplyInsets(this)
        else -> doOnAttach(ViewCompat::requestApplyInsets)
    }
}

fun WindowInsetsCompat.topInset(): Int = getInsets(Type.systemBars() or Type.displayCutout()).top

fun WindowInsetsCompat.navigationInset(): Int = getInsets(Type.systemBars()).bottom

fun WindowInsetsCompat.bottomObstruction(): Int = maxOf(getInsets(Type.ime()).bottom, navigationInset())

fun WindowInsetsCompat.horizontalInsets(): Insets = getInsets(Type.systemBars() or Type.displayCutout())

fun WindowInsetsCompat.keyboardVisible(): Boolean = isVisible(Type.ime())

fun WindowInsetsCompat.underChrome(top: Int, bottom: Int): WindowInsetsCompat {
    val status = getInsets(Type.statusBars())
    val nav = getInsets(Type.navigationBars())
    return WindowInsetsCompat.Builder(this)
        .setInsets(Type.statusBars(), Insets.of(status.left, top, status.right, 0))
        .setInsets(Type.captionBar(), Insets.NONE)
        .setInsets(Type.navigationBars(), Insets.of(nav.left, 0, nav.right, bottom))
        .build()
}

fun View.applySystemBarsPadding() = doOnApplyInsets { view, insets, initial ->
    val sides = insets.horizontalInsets()
    view.updatePadding(
        left = initial.left + sides.left,
        top = initial.top + insets.topInset(),
        right = initial.right + sides.right,
        bottom = initial.bottom + insets.bottomObstruction(),
    )
}

fun View.applyContentInsets(spec: ContentInsets = ContentInsets()) {
    (this as? ViewGroup)?.clipToPadding = false
    val insets = ContentInsetsListener(this, spec)
    (this as? RecyclerView)?.let(insets::bindSticky)
    ViewCompat.setOnApplyWindowInsetsListener(this, insets)
    requestInsetsWhenAttached()
}

fun View.pinAboveKeyboard(content: View) {
    val baseMargin = marginBottom
    doOnApplyInsets { panel, insets, _ ->
        panel.updateLayoutParams<ViewGroup.MarginLayoutParams> {
            bottomMargin = baseMargin + insets.bottomObstruction()
        }
    }
    addOnLayoutChangeListener(PanelSpacer(content, content.paddingBottom))
    ViewCompat.setWindowInsetsAnimationCallback(this, KeyboardFollower(this))
    (content as? NestedScrollView)?.keepFocusedVisible()
}

fun NestedScrollView.keepFocusedVisible() {
    val listener = ViewTreeObserver.OnGlobalFocusChangeListener { _, focused -> post { reveal(focused) } }
    doOnAttach { viewTreeObserver.addOnGlobalFocusChangeListener(listener) }
    doOnDetach { viewTreeObserver.removeOnGlobalFocusChangeListener(listener) }
}

fun NestedScrollView.revealFocused() {
    reveal(findFocus())
}

private fun NestedScrollView.reveal(target: View?) {
    val owner = target?.revealTarget(this) ?: return
    val rect = Rect()
    owner.getDrawingRect(rect)
    offsetDescendantRectToMyCoords(owner, rect)
    val visibleTop = scrollY + paddingTop
    val visibleBottom = scrollY + height - paddingBottom
    val delta = when {
        rect.bottom > visibleBottom -> rect.bottom - visibleBottom
        rect.top < visibleTop -> rect.top - visibleTop
        else -> 0
    }
    when {
        delta != 0 -> smoothScrollBy(0, delta)
    }
}

private fun View.revealTarget(scroll: NestedScrollView): View? = when {
    !scroll.isAncestorOf(this) -> null
    else -> generateSequence(this) { it.parent as? View }
        .takeWhile { it !== scroll }
        .firstOrNull { it is TextInputLayout } ?: this
}

private fun View.isAncestorOf(child: View): Boolean =
    generateSequence(child.parent) { it.parent }.any { it === this }

private class PanelSpacer(
    private val content: View,
    private val initialBottom: Int,
) : View.OnLayoutChangeListener {
    override fun onLayoutChange(
        panel: View,
        left: Int,
        top: Int,
        right: Int,
        bottom: Int,
        oldLeft: Int,
        oldTop: Int,
        oldRight: Int,
        oldBottom: Int,
    ) {
        val wanted = initialBottom + panel.height + panel.marginBottom
        when {
            content.paddingBottom != wanted -> content.post { applySpace(wanted) }
        }
    }

    private fun applySpace(wanted: Int) {
        content.updatePadding(bottom = wanted)
        (content as? NestedScrollView)?.post { content.revealFocused() }
    }
}

private class KeyboardFollower(
    private val panel: View,
) : WindowInsetsAnimationCompat.Callback(DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
    private var startGap = 0
    private var restingGap = 0

    override fun onPrepare(animation: WindowInsetsAnimationCompat) {
        panel.translationY = 0f
        startGap = panel.gapToWindowBottom()
    }

    override fun onStart(
        animation: WindowInsetsAnimationCompat,
        bounds: WindowInsetsAnimationCompat.BoundsCompat,
    ): WindowInsetsAnimationCompat.BoundsCompat {
        restingGap = panel.gapToWindowBottom()
        return bounds
    }

    override fun onProgress(
        insets: WindowInsetsCompat,
        runningAnimations: MutableList<WindowInsetsAnimationCompat>,
    ): WindowInsetsCompat {
        val keyboard = runningAnimations.any { it.typeMask and Type.ime() != 0 }
        val floor = when {
            restingGap < startGap -> restingGap
            else -> insets.navigationInset()
        }
        val current = maxOf(insets.getInsets(Type.ime()).bottom, floor)
        panel.translationY = when {
            keyboard -> (restingGap - current).toFloat()
            else -> 0f
        }
        return insets
    }

    override fun onEnd(animation: WindowInsetsAnimationCompat) {
        panel.translationY = 0f
    }
}

internal fun View.gapToWindowBottom(): Int {
    val location = IntArray(2)
    getLocationInWindow(location)
    return rootView.height - (location[1] + height - translationY.toInt())
}

fun View.liftAboveBottomInsets() {
    val baseMargin = marginBottom
    doOnApplyInsets { view, insets, _ ->
        view.updateLayoutParams<ViewGroup.MarginLayoutParams> {
            bottomMargin = baseMargin + insets.bottomObstruction()
        }
    }
}

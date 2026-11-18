package dev.proflyder.home.core.ui

import android.annotation.SuppressLint
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.sign

/**
 * Drag-to-dismiss for a notice bar: a sideways or downward swipe drags the bar
 * with the finger and dismisses it once the gesture passes a fraction of its
 * size. A tap (below the touch slop) is ignored, so an action button inside the
 * bar keeps working, and an upward drag springs back instead of dismissing.
 */
internal class NoticeSwipeDismiss(
    private val view: View,
    private val onDismiss: () -> Unit,
) : View.OnTouchListener {
    private val touchSlop = ViewConfiguration.get(view.context).scaledTouchSlop
    private var startX = 0f
    private var startY = 0f
    private var offsetX = 0f
    private var offsetY = 0f
    private var dragging = false
    private var dismissing = false

    fun attach() {
        view.setOnTouchListener(this)
    }

    // A tap has no click action of its own here: Material's SnackbarBaseLayout
    // also swallows all touches, and an action button inside the bar is a
    // separate clickable child that keeps receiving its own clicks.
    @SuppressLint("ClickableViewAccessibility")
    override fun onTouch(target: View, event: MotionEvent): Boolean = when (dismissing) {
        true -> true
        false -> handle(event)
    }

    private fun handle(event: MotionEvent): Boolean = when (event.actionMasked) {
        MotionEvent.ACTION_DOWN -> press(event)
        MotionEvent.ACTION_MOVE -> move(event)
        MotionEvent.ACTION_UP -> release()
        MotionEvent.ACTION_CANCEL -> cancel()
        else -> true
    }

    private fun press(event: MotionEvent): Boolean {
        startX = event.rawX
        startY = event.rawY
        offsetX = 0f
        offsetY = 0f
        dragging = false
        return true
    }

    private fun move(event: MotionEvent): Boolean {
        val dx = event.rawX - startX
        val dy = event.rawY - startY
        dragging = dragging || hypot(dx, dy) >= touchSlop
        when (dragging) {
            true -> {
                view.animate().cancel()
                dragTo(dx, dy.coerceAtLeast(0f))
            }
            false -> Unit
        }
        return true
    }

    private fun dragTo(dx: Float, dy: Float) {
        offsetX = dx
        offsetY = dy
        view.translationX = dx
        view.translationY = dy
        view.alpha = 1f - progress(dx, dy) * DRAG_FADE
    }

    private fun release(): Boolean {
        when {
            !dragging -> Unit
            pastThreshold() -> dismiss()
            else -> settle()
        }
        dragging = false
        return true
    }

    private fun cancel(): Boolean {
        when {
            dragging -> settle()
            else -> Unit
        }
        dragging = false
        return true
    }

    private fun pastThreshold(): Boolean =
        abs(offsetX) >= width() * DISMISS_HORIZONTAL_FRACTION ||
            offsetY >= height() * DISMISS_VERTICAL_FRACTION

    private fun dismiss() {
        dismissing = true
        view.animate()
            .translationX(exitX())
            .translationY(exitY())
            .alpha(0f)
            .setDuration(EXIT_DURATION_MS)
            .withEndAction(onDismiss)
            .start()
    }

    private fun settle() {
        view.animate()
            .translationX(0f)
            .translationY(0f)
            .alpha(1f)
            .setDuration(SETTLE_DURATION_MS)
            .start()
    }

    private fun exitX(): Float = when {
        abs(offsetX) >= width() * DISMISS_HORIZONTAL_FRACTION -> sign(offsetX) * width()
        else -> offsetX
    }

    private fun exitY(): Float = when {
        offsetY >= height() * DISMISS_VERTICAL_FRACTION -> height()
        else -> offsetY
    }

    private fun progress(dx: Float, dy: Float): Float =
        maxOf(abs(dx) / width(), dy / height()).coerceIn(0f, 1f)

    private fun width(): Float =
        view.width.takeIf { it > 0 }?.toFloat() ?: view.resources.displayMetrics.widthPixels.toFloat()

    private fun height(): Float =
        view.height.takeIf { it > 0 }?.toFloat() ?: view.resources.displayMetrics.heightPixels.toFloat()

    private companion object {
        const val DISMISS_HORIZONTAL_FRACTION = 0.25f
        const val DISMISS_VERTICAL_FRACTION = 0.25f
        const val DRAG_FADE = 0.4f
        const val EXIT_DURATION_MS = 160L
        const val SETTLE_DURATION_MS = 150L
    }
}

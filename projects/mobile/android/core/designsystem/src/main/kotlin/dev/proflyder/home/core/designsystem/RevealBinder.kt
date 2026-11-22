package dev.proflyder.home.core.designsystem

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.ValueAnimator
import android.view.View
import android.view.ViewGroup
import androidx.core.view.isVisible

class RevealBinder(private val view: View) {
    internal var animator: ValueAnimator? = null
        private set
    private var shown: Boolean = view.isVisible

    fun render(visible: Boolean) {
        if (visible == shown) return
        shown = visible
        animator?.cancel()
        if (!canAnimate()) return snap(visible)
        if (visible) reveal() else conceal()
    }

    private fun canAnimate(): Boolean =
        view.isAttachedToWindow && (view.parent as? View)?.isLaidOut == true && ValueAnimator.areAnimatorsEnabled() && ((view.parent as? View)?.width ?: 0) > 0

    private fun snap(visible: Boolean) {
        animator = null
        view.layoutParams = view.layoutParams.apply { height = ViewGroup.LayoutParams.WRAP_CONTENT }
        view.alpha = 1f
        view.isVisible = visible
    }

    private fun reveal() {
        val from = if (view.isVisible) view.height else 0
        view.isVisible = true
        if (from == 0) view.alpha = 0f
        animate(from, targetHeight(), 1f, HomeMotion.enterDuration(view.context), visibleAtEnd = true)
    }

    private fun conceal() {
        animate(view.height, 0, 0f, HomeMotion.exitDuration(view.context), visibleAtEnd = false)
    }

    private fun animate(from: Int, to: Int, alpha: Float, duration: Int, visibleAtEnd: Boolean) {
        val startAlpha = view.alpha
        animator = ValueAnimator.ofFloat(0f, 1f).apply {
            this.duration = duration.toLong()
            interpolator = HomeMotion.selectionInterpolator(view.context)
            addUpdateListener { step(from, to, startAlpha, alpha, it.animatedFraction) }
            addListener(EndListener { snap(visibleAtEnd) })
            start()
        }
    }

    private fun step(from: Int, to: Int, startAlpha: Float, endAlpha: Float, fraction: Float) {
        view.layoutParams = view.layoutParams.apply { height = from + ((to - from) * fraction).toInt() }
        view.alpha = startAlpha + (endAlpha - startAlpha) * fraction
    }

    private fun targetHeight(): Int {
        val width = (view.parent as View).width
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        return view.measuredHeight
    }

    private class EndListener(private val onEnd: () -> Unit) : AnimatorListenerAdapter() {
        private var cancelled = false

        override fun onAnimationCancel(animation: Animator) {
            cancelled = true
        }

        override fun onAnimationEnd(animation: Animator) {
            if (!cancelled) onEnd()
        }
    }
}

package dev.proflyder.home.core.ui

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.ValueAnimator
import android.view.View
import dev.proflyder.home.core.designsystem.HomeMotion
import kotlin.math.roundToInt

fun View.animateBar(visible: Boolean, hiddenTranslationY: Float, animate: Boolean = true) {
    animate().withEndAction(null)
    animate().cancel()

    val targetTranslation = if (visible) 0f else hiddenTranslationY
    val targetAlpha = if (visible) 1f else 0f
    val targetVisibility = if (visible) View.VISIBLE else View.GONE

    val unchanged = visibility == targetVisibility &&
        translationY == targetTranslation &&
        alpha == targetAlpha
    if (!animate || !ValueAnimator.areAnimatorsEnabled() || unchanged) {
        translationY = targetTranslation
        alpha = targetAlpha
        visibility = targetVisibility
        return
    }

    if (visible && visibility != View.VISIBLE) {
        visibility = View.VISIBLE
        translationY = hiddenTranslationY
        alpha = 0f
    }

    val context = context
    val animation = animate()
        .translationY(targetTranslation)
        .alpha(targetAlpha)
        .setDuration(
            (if (visible) HomeMotion.enterDuration(context) else HomeMotion.exitDuration(context)).toLong(),
        )
        .setInterpolator(
            if (visible) HomeMotion.enterInterpolator(context) else HomeMotion.exitInterpolator(context),
        )
    if (!visible) {
        animation.withEndAction {
            visibility = View.GONE
            translationY = hiddenTranslationY
            alpha = 0f
        }
    }
    animation.start()
}

fun View.animateVerticalPadding(
    topPx: Int,
    bottomPx: Int,
    entering: Boolean,
    animate: Boolean = true,
) {
    val startTop = paddingTop
    val startBottom = paddingBottom
    (getTag(R.id.ui_motion_padding_animator) as? ValueAnimator)?.cancel()
    setTag(R.id.ui_motion_padding_animator, null)

    val unchanged = startTop == topPx && startBottom == bottomPx
    if (!animate || !ValueAnimator.areAnimatorsEnabled() || unchanged) {
        setPadding(paddingLeft, topPx, paddingRight, bottomPx)
        return
    }

    val context = context
    val animator = ValueAnimator.ofFloat(0f, 1f).apply {
        duration = (if (entering) HomeMotion.enterDuration(context) else HomeMotion.exitDuration(context)).toLong()
        interpolator = if (entering) {
            HomeMotion.enterInterpolator(context)
        } else {
            HomeMotion.exitInterpolator(context)
        }
        addUpdateListener { animation ->
            val fraction = animation.animatedFraction
            setPadding(
                paddingLeft,
                (startTop + (topPx - startTop) * fraction).roundToInt(),
                paddingRight,
                (startBottom + (bottomPx - startBottom) * fraction).roundToInt(),
            )
        }
        addListener(
            object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: Animator) {
                    if (getTag(R.id.ui_motion_padding_animator) === animation) {
                        setTag(R.id.ui_motion_padding_animator, null)
                    }
                }
            },
        )
    }
    setTag(R.id.ui_motion_padding_animator, animator)
    animator.start()
}

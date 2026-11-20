package dev.proflyder.home.core.ui

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.TimeInterpolator
import android.animation.ValueAnimator
import android.content.Context
import android.view.View
import android.view.animation.PathInterpolator
import com.google.android.material.R as MaterialR
import com.google.android.material.motion.MotionUtils
import kotlin.math.roundToInt

object HomeMotion {
    private const val FALLBACK_ENTER_MS = 300
    private const val FALLBACK_EXIT_MS = 200
    private const val FALLBACK_SELECTION_MS = 150

    private val fallbackDecelerate: TimeInterpolator = PathInterpolator(0.05f, 0.7f, 0.1f, 1f)

    private val fallbackAccelerate: TimeInterpolator = PathInterpolator(0.3f, 0f, 0.8f, 0.15f)

    fun enterDuration(context: Context): Int =
        MotionUtils.resolveThemeDuration(
            context,
            MaterialR.attr.motionDurationMedium2,
            FALLBACK_ENTER_MS,
        )

    fun exitDuration(context: Context): Int =
        MotionUtils.resolveThemeDuration(
            context,
            MaterialR.attr.motionDurationShort4,
            FALLBACK_EXIT_MS,
        )

    fun selectionDuration(context: Context): Int =
        MotionUtils.resolveThemeDuration(
            context,
            MaterialR.attr.motionDurationShort3,
            FALLBACK_SELECTION_MS,
        )

    fun enterInterpolator(context: Context): TimeInterpolator =
        MotionUtils.resolveThemeInterpolator(
            context,
            MaterialR.attr.motionEasingEmphasizedDecelerateInterpolator,
            fallbackDecelerate,
        )

    fun exitInterpolator(context: Context): TimeInterpolator =
        MotionUtils.resolveThemeInterpolator(
            context,
            MaterialR.attr.motionEasingEmphasizedAccelerateInterpolator,
            fallbackAccelerate,
        )

    fun selectionInterpolator(context: Context): TimeInterpolator = enterInterpolator(context)
}

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

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

/**
 * Material 3 motion used by in-screen bars and selection affordances.
 *
 * Durations and easing come from the Material theme so they follow the app's
 * motion system; the fallbacks match the M3 baseline if an attribute is
 * missing.
 */
object HomeMotion {
    private const val FALLBACK_ENTER_MS = 300
    private const val FALLBACK_EXIT_MS = 200
    private const val FALLBACK_SELECTION_MS = 150

    /** M3 emphasized decelerate: cubic-bezier(0.05, 0.7, 0.1, 1.0). */
    private val fallbackDecelerate: TimeInterpolator = PathInterpolator(0.05f, 0.7f, 0.1f, 1f)

    /** M3 emphasized accelerate: cubic-bezier(0.3, 0.0, 0.8, 0.15). */
    private val fallbackAccelerate: TimeInterpolator = PathInterpolator(0.3f, 0f, 0.8f, 0.15f)

    /** Emphasized decelerate duration for something entering the screen. */
    fun enterDuration(context: Context): Int =
        MotionUtils.resolveThemeDuration(
            context,
            MaterialR.attr.motionDurationMedium2,
            FALLBACK_ENTER_MS,
        )

    /** Emphasized accelerate duration for something leaving the screen. */
    fun exitDuration(context: Context): Int =
        MotionUtils.resolveThemeDuration(
            context,
            MaterialR.attr.motionDurationShort4,
            FALLBACK_EXIT_MS,
        )

    /** Short duration for an in-place state change such as a selection. */
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

/**
 * Shows or hides a bar with a vertical slide and fade.
 *
 * The animation starts from the view's current translation and alpha, so a
 * rapid toggle cancels the previous animator and continues from where it was
 * instead of snapping or jumping. [hiddenTranslationY] is the offset used while
 * the bar is hidden: negative slides in from above, positive from below.
 *
 * @param animate false jumps to the end state without animating. This is used
 *   when restoring state and when the system disables animations.
 */
fun View.animateBar(visible: Boolean, hiddenTranslationY: Float, animate: Boolean = true) {
    // Clear the pending end action first: cancelling must not run the previous
    // exit's finalizer, so an interrupted exit keeps its current translation
    // and alpha and the new animation continues from there.
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

/**
 * Animates the vertical padding of [this] to make room for an overlaying bar
 * without resizing or jumping the content.
 *
 * The animation starts from the current padding, so an interrupted enter/exit
 * continues smoothly. [entering] selects the enter or exit motion curve.
 *
 * @param animate false jumps to the end state without animating.
 */
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

package dev.proflyder.home.core.designsystem

import android.animation.TimeInterpolator
import android.content.Context
import android.view.animation.PathInterpolator
import com.google.android.material.R as MaterialR
import com.google.android.material.motion.MotionUtils

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

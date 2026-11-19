package dev.proflyder.home.core.theme

import androidx.annotation.ColorInt
import kotlin.math.pow

object Contrast {
    const val AA_TEXT = 4.5

    fun ratio(@ColorInt first: Int, @ColorInt second: Int): Double {
        val a = luminance(first)
        val b = luminance(second)
        return (maxOf(a, b) + FLARE) / (minOf(a, b) + FLARE)
    }

    private fun luminance(@ColorInt color: Int): Double =
        RED * channel(color shr RED_SHIFT) + GREEN * channel(color shr GREEN_SHIFT) + BLUE * channel(color)

    private fun channel(value: Int): Double {
        val srgb = (value and CHANNEL_MASK) / CHANNEL_MAX
        return when {
            srgb <= LINEAR_LIMIT -> srgb / LINEAR_DIVISOR
            else -> ((srgb + GAMMA_OFFSET) / GAMMA_DIVISOR).pow(GAMMA)
        }
    }

    private const val FLARE = 0.05
    private const val RED = 0.2126
    private const val GREEN = 0.7152
    private const val BLUE = 0.0722
    private const val RED_SHIFT = 16
    private const val GREEN_SHIFT = 8
    private const val CHANNEL_MASK = 0xFF
    private const val CHANNEL_MAX = 255.0
    private const val LINEAR_LIMIT = 0.04045
    private const val LINEAR_DIVISOR = 12.92
    private const val GAMMA_OFFSET = 0.055
    private const val GAMMA_DIVISOR = 1.055
    private const val GAMMA = 2.4
}

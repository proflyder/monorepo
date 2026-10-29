package dev.proflyder.home.feature.control

import kotlin.math.ln
import kotlin.math.pow

/** Pure color math for previewing a light on the phone (ARGB ints, no Android types). */
object LightColors {
    /** Approximate color of a white light at [kelvin] (1000-40000), opaque ARGB. */
    fun kelvinToArgb(kelvin: Int): Int {
        val t = kelvin.coerceIn(1000, 40000) / 100.0
        val red = if (t <= 66) 255.0 else 329.698727446 * (t - 60).pow(-0.1332047592)
        val green = if (t <= 66) 99.4708025861 * ln(t) - 161.1195681661 else 288.1221695283 * (t - 60).pow(-0.0755148492)
        val blue = when {
            t >= 66 -> 255.0
            t <= 19 -> 0.0
            else -> 138.5177312231 * ln(t - 10) - 305.0447927307
        }
        return argb(red.toChannel(), green.toChannel(), blue.toChannel())
    }

    /** Fully bright color for [hue] (0-360) and [saturation] (0-100), opaque ARGB. */
    fun hsToArgb(hue: Int, saturation: Int): Int {
        val h = ((hue % 360) + 360) % 360 / 60.0
        val s = saturation.coerceIn(0, 100) / 100.0
        val x = 1 - kotlin.math.abs(h % 2 - 1)
        val (r, g, b) = when (h.toInt()) {
            0 -> Triple(1.0, x, 0.0)
            1 -> Triple(x, 1.0, 0.0)
            2 -> Triple(0.0, 1.0, x)
            3 -> Triple(0.0, x, 1.0)
            4 -> Triple(x, 0.0, 1.0)
            else -> Triple(1.0, 0.0, x)
        }
        fun mix(c: Double) = ((c * s + (1 - s)) * 255).toChannel()
        return argb(mix(r), mix(g), mix(b))
    }

    private fun Double.toChannel(): Int = coerceIn(0.0, 255.0).toInt()

    private fun argb(r: Int, g: Int, b: Int): Int = (0xFF shl 24) or (r shl 16) or (g shl 8) or b
}

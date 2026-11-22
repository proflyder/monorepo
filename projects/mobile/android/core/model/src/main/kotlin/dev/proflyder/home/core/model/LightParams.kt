package dev.proflyder.home.core.model

import kotlin.math.roundToInt

data class LightParams(
    val brightnessPct: Int? = null,
    val kelvin: Int? = null,
    val effect: String? = null,
    val transitionSeconds: Double? = null,
)

data class LightRanges(
    val brightness: IntRange?,
    val kelvin: IntRange?,
) {
    fun brightnessOf(value: Int?, fallback: Int): Int? = brightness?.let { (value ?: fallback).coerceIn(it) }

    fun kelvinOf(value: Int?, fallback: Int): Int? {
        val range = kelvin ?: return null
        val wanted = value?.takeIf { it > 0 } ?: fallback.takeIf { it > 0 } ?: (range.first + range.last) / 2
        return roundKelvin(wanted).coerceIn(range)
    }

    companion object {
        const val KELVIN_STEP = 50
        const val MIN_BRIGHTNESS = 1
        const val MAX_BRIGHTNESS = 100
        const val MAX_TRANSITION_SECONDS = 300.0
        private const val MIN_KELVIN = 1000

        fun of(controls: LightControls): LightRanges = LightRanges(
            brightness = if (controls.brightness) MIN_BRIGHTNESS..MAX_BRIGHTNESS else null,
            kelvin = if (controls.colorTemp) kelvinRange(controls.minKelvin, controls.maxKelvin) else null,
        )

        fun roundKelvin(value: Int): Int = (value / KELVIN_STEP.toDouble()).roundToInt() * KELVIN_STEP

        fun transitionOf(raw: String): Double? = raw.trim().toDoubleOrNull()
            ?.takeIf { it.isFinite() && it in 0.0..MAX_TRANSITION_SECONDS }

        private fun kelvinRange(min: Int, max: Int): IntRange {
            val low = roundKelvin(min.coerceAtLeast(MIN_KELVIN))
            val high = roundKelvin(max).coerceAtLeast(low + KELVIN_STEP)
            return low..high
        }
    }
}

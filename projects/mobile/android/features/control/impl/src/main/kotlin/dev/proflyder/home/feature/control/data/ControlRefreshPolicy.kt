package dev.proflyder.home.feature.control.data

import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

internal data class ControlRefreshPolicy(
    val period: Duration = 30.seconds,
    val stopTimeout: Duration = 5.seconds,
    val expectationTtl: Duration = 15.seconds,
    val powerSettle: List<Duration> = List(POWER_SETTLE_STEPS) { SETTLE_STEP },
    val volumeSettle: List<Duration> = listOf(SETTLE_STEP),
    val pressSettle: List<Duration> = listOf(Duration.ZERO, PRESS_SETTLE),
) {
    private companion object {
        const val POWER_SETTLE_STEPS = 8
        val SETTLE_STEP = 1_500.milliseconds
        val PRESS_SETTLE = 4_500.milliseconds
    }
}

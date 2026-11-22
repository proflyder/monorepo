package dev.proflyder.home.feature.timers.presentation

import dev.proflyder.home.core.model.Clock
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

internal class TimerRefreshPolicy(
    private val clock: Clock,
    private val periodic: Duration = 30.seconds,
    private val afterExpiry: Duration = 10.seconds,
) {
    private var lastRefreshNanos = clock.elapsedNanos()
    private var lastExpiryNanos: Long? = null

    fun refreshed() {
        lastRefreshNanos = clock.elapsedNanos()
    }

    fun due(rows: List<TimerRow>): Boolean {
        val now = clock.elapsedNanos()
        val expired = rows.any { it.expired }
        val expiryDue = expired && lastExpiryNanos.olderThan(now, afterExpiry)
        if (expiryDue) lastExpiryNanos = now
        return expiryDue || now - lastRefreshNanos >= periodic.inWholeNanoseconds
    }

    private fun Long?.olderThan(now: Long, window: Duration): Boolean =
        this == null || now - this >= window.inWholeNanoseconds
}

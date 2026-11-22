package dev.proflyder.home.core.testing

import dev.proflyder.home.core.model.Clock

class FakeClock(
    var wallMillis: Long = 0L,
    var elapsedNanos: Long = 0L,
) : Clock {
    override fun wallMillis(): Long = wallMillis

    override fun elapsedNanos(): Long = elapsedNanos

    fun advanceMillis(millis: Long) {
        wallMillis += millis
        elapsedNanos += millis * NANOS_PER_MILLI
    }

    private companion object {
        const val NANOS_PER_MILLI = 1_000_000L
    }
}

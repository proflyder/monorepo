package dev.proflyder.home.core.model

class ServerClock(private val clock: Clock) {
    @Volatile
    private var anchorElapsedNanos: Long = 0L

    @Volatile
    private var anchorServerUnix: Long = 0L

    @Volatile
    private var synced: Boolean = false

    val isSynced: Boolean get() = synced

    fun sync(serverNowUnix: Long) {
        if (serverNowUnix <= 0L) return
        anchorServerUnix = serverNowUnix
        anchorElapsedNanos = clock.elapsedNanos()
        synced = true
    }

    fun nowUnix(): Long {
        if (!synced) return clock.wallMillis() / 1000L
        val elapsedNanosNow = clock.elapsedNanos()
        val elapsedSinceSync = (elapsedNanosNow - anchorElapsedNanos).coerceAtLeast(0L)
        return anchorServerUnix + elapsedSinceSync / NANOS_PER_SECOND
    }

    fun offsetSeconds(): Long = nowUnix() - clock.wallMillis() / 1000L

    fun remainingSeconds(timer: Timer): Long = when (timer.state) {
        TimerState.RUNNING -> (timer.dueUnix - nowUnix()).coerceAtLeast(0L)
        TimerState.PAUSED -> timer.remainingSeconds.coerceAtLeast(0L)
        else -> 0L
    }

    private companion object {
        const val NANOS_PER_SECOND = 1_000_000_000L
    }
}

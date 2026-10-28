package dev.proflyder.home.core.model

/**
 * Maps the phone onto the scheduler's clock.
 *
 * After a successful GetStatus, the server timestamp is anchored to a monotonic
 * elapsed-time source (Android `elapsedRealtimeNanos` in the app, `nanoTime` in
 * tests). A later wall-clock correction therefore cannot move countdowns; only
 * elapsed time advances them, and a new sync corrects any server offset.
 *
 * Before the first sync the phone wall clock is the only available fallback.
 */
class ServerClock(
    private val localMillis: () -> Long = System::currentTimeMillis,
    private val elapsedNanos: () -> Long = System::nanoTime,
) {
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
        anchorElapsedNanos = elapsedNanos()
        synced = true
    }

    fun nowUnix(): Long {
        if (!synced) return localMillis() / 1000L
        val elapsedNanosNow = elapsedNanos()
        val elapsedSinceSync = (elapsedNanosNow - anchorElapsedNanos).coerceAtLeast(0L)
        return anchorServerUnix + elapsedSinceSync / NANOS_PER_SECOND
    }

    /** Current phone-vs-server skew; diagnostics only. */
    fun offsetSeconds(): Long = nowUnix() - localMillis() / 1000L

    fun remainingSeconds(timer: Timer): Long = when (timer.state) {
        TimerState.RUNNING -> (timer.dueUnix - nowUnix()).coerceAtLeast(0L)
        TimerState.PAUSED -> timer.remainingSeconds.coerceAtLeast(0L)
        else -> 0L
    }

    private companion object {
        const val NANOS_PER_SECOND = 1_000_000_000L
    }
}

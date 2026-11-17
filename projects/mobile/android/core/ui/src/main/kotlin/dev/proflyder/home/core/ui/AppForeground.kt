package dev.proflyder.home.core.ui

/**
 * When the app last returned to the foreground. Waking the phone races the
 * Wi-Fi/VPN tunnel: a transient network failure in the first seconds after a
 * resume is expected and must not become a blocking error notice.
 */
object AppForeground {
    /** How long after a resume transient network failures stay quiet. */
    const val GRACE_NANOS: Long = 6_000_000_000L

    private const val NEVER = Long.MIN_VALUE / 2

    @Volatile
    private var resumedAtNanos: Long = NEVER

    /** Called from the activity every time it returns to the foreground. */
    fun onResumed(nowNanos: Long = System.nanoTime()) {
        resumedAtNanos = nowNanos
    }

    fun inGrace(nowNanos: Long = System.nanoTime()): Boolean =
        nowNanos - resumedAtNanos in 0..GRACE_NANOS
}

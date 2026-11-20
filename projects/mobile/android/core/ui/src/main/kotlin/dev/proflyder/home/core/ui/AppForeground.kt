package dev.proflyder.home.core.ui

object AppForeground {
    const val GRACE_NANOS: Long = 6_000_000_000L

    private const val NEVER = Long.MIN_VALUE / 2

    @Volatile
    private var resumedAtNanos: Long = NEVER

    fun onResumed(nowNanos: Long = System.nanoTime()) {
        resumedAtNanos = nowNanos
    }

    fun inGrace(nowNanos: Long = System.nanoTime()): Boolean =
        nowNanos - resumedAtNanos in 0..GRACE_NANOS
}

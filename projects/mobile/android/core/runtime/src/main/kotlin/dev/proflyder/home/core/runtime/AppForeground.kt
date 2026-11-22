package dev.proflyder.home.core.runtime

import kotlinx.coroutines.flow.StateFlow

interface AppForeground {
    val state: StateFlow<ForegroundState>
}

data class ForegroundState(
    val foreground: Boolean,
    val sinceElapsedNanos: Long,
) {
    fun foregroundFor(nowElapsedNanos: Long): Long? =
        (nowElapsedNanos - sinceElapsedNanos).takeIf { foreground }
}

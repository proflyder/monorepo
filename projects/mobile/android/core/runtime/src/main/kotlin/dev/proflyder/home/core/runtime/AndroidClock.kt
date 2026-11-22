package dev.proflyder.home.core.runtime

import android.os.SystemClock
import dev.proflyder.home.core.model.Clock

object AndroidClock : Clock {
    override fun wallMillis(): Long = System.currentTimeMillis()

    override fun elapsedNanos(): Long = SystemClock.elapsedRealtimeNanos()
}

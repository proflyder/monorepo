package dev.proflyder.home.feature.timers

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Timer

/** Domain contract for server-owned countdown timers. */
interface TimerRepository {
    suspend fun list(): Outcome<List<Timer>>

    /** Syncs the shared ServerClock from GetStatus before displaying countdowns. */
    suspend fun syncClock(): Outcome<Unit>
    suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer>
    suspend fun pause(id: String): Outcome<Timer>
    suspend fun resume(id: String): Outcome<Timer>
    suspend fun cancel(id: String): Outcome<Timer>
}

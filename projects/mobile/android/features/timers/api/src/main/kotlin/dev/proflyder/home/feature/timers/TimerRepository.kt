package dev.proflyder.home.feature.timers

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Timer

interface TimerRepository : EntitySource<String, Timer> {
    suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer>
    suspend fun pause(id: String): Outcome<Timer>
    suspend fun resume(id: String): Outcome<Timer>
    suspend fun cancel(id: String): Outcome<Timer>
}

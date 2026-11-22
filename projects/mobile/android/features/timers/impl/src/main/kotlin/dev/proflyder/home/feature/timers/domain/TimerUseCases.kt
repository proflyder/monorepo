package dev.proflyder.home.feature.timers.domain

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.timers.TimerRepository
import dev.zacsweers.metro.Inject

@Inject class LoadTimers(
    private val repository: TimerRepository,
    private val serverClock: ServerClockSync,
) {
    suspend operator fun invoke(): Outcome<List<Timer>> {
        when (val sync = serverClock.sync()) {
            is Outcome.Failure -> return sync
            is Outcome.Success -> Unit
        }
        return repository.list()
    }
}

@Inject class StartTimer(
    private val repository: TimerRepository,
) {
    suspend operator fun invoke(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> {
        if (durationSeconds !in DurationText.MIN_SECONDS..DurationText.MAX_SECONDS) {
            return Outcome.Failure(RemoteFailure.Validation(Field.DURATION))
        }
        return repository.start(label, durationSeconds, idempotencyKey)
    }
}

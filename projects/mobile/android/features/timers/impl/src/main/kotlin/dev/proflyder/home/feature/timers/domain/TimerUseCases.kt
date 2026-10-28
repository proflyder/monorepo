package dev.proflyder.home.feature.timers.domain

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.timers.TimerRepository
import dev.zacsweers.metro.Inject

class LoadTimers @Inject constructor(
    private val repository: TimerRepository,
) {
    /** Syncs server time first so remaining values are not derived from a wrong phone clock. */
    suspend operator fun invoke(): Outcome<List<Timer>> {
        when (val sync = repository.syncClock()) {
            is Outcome.Failure -> return sync
            is Outcome.Success -> Unit
        }
        return repository.list()
    }
}

class StartTimer @Inject constructor(
    private val repository: TimerRepository,
) {
    suspend operator fun invoke(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> {
        if (durationSeconds !in DurationText.MIN_SECONDS..DurationText.MAX_SECONDS) {
            return Outcome.Failure(RemoteFailure.Validation(Field.DURATION))
        }
        return repository.start(label, durationSeconds, idempotencyKey)
    }
}

class PauseTimer @Inject constructor(
    private val repository: TimerRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Timer> = repository.pause(id)
}

class ResumeTimer @Inject constructor(
    private val repository: TimerRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Timer> = repository.resume(id)
}

class CancelTimer @Inject constructor(
    private val repository: TimerRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Timer> = repository.cancel(id)
}

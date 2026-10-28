package dev.proflyder.home.feature.timers.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.feature.timers.TimerRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectTimerRepository(
    private val client: SchedulerClient,
) : TimerRepository {
    override suspend fun list(): Outcome<List<Timer>> = client.listTimers(includeFinished = false)

    override suspend fun syncClock(): Outcome<Unit> = when (val result = client.getStatus()) {
        is Outcome.Success -> Outcome.Success(Unit)
        is Outcome.Failure -> result
    }

    override suspend fun start(
        label: String,
        durationSeconds: Long,
        idempotencyKey: String,
    ): Outcome<Timer> = client.startTimer(
        label = label.trim().ifEmpty { "Timer" },
        durationSeconds = durationSeconds,
        idempotencyKey = idempotencyKey,
    )

    override suspend fun pause(id: String): Outcome<Timer> = client.pauseTimer(id)

    override suspend fun resume(id: String): Outcome<Timer> = client.resumeTimer(id)

    override suspend fun cancel(id: String): Outcome<Timer> = client.cancelTimer(id)
}

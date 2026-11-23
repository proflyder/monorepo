package dev.proflyder.home.feature.timers.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.network.SchedulerClient
import dev.zacsweers.metro.Inject

internal interface TimerRemote {
    suspend fun list(): Outcome<List<Timer>>
    suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer>
    suspend fun pause(id: String): Outcome<Timer>
    suspend fun resume(id: String): Outcome<Timer>
    suspend fun cancel(id: String): Outcome<Timer>
}

@Inject
internal class ConnectTimerRemote(
    private val client: SchedulerClient,
    private val serverClock: ServerClockSync,
) : TimerRemote {
    override suspend fun list(): Outcome<List<Timer>> = when (val sync = serverClock.sync()) {
        is Outcome.Failure -> sync
        is Outcome.Success -> client.listTimers(includeFinished = false)
    }

    override suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> =
        client.startTimer(label = label.trim(), durationSeconds = durationSeconds, idempotencyKey = idempotencyKey)

    override suspend fun pause(id: String): Outcome<Timer> = client.pauseTimer(id)

    override suspend fun resume(id: String): Outcome<Timer> = client.resumeTimer(id)

    override suspend fun cancel(id: String): Outcome<Timer> = client.cancelTimer(id)
}

package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.model.map

class SchedulerServerClockSync(
    private val client: SchedulerClient,
) : ServerClockSync {
    override suspend fun sync(): Outcome<Unit> = client.getStatus().map { }
}

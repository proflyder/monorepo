package dev.proflyder.home.core.model

interface ServerClockSync {
    suspend fun sync(): Outcome<Unit>
}

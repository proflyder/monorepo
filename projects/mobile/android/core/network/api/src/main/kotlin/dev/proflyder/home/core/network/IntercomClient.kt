package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.IntercomCall
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomStatus
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.Flow

interface IntercomClient {
    suspend fun registerEndpoint(displayName: String, capabilities: List<String>): Outcome<IntercomEndpoint>
    suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>>
    suspend fun startCall(calleeEndpointId: String, idempotencyKey: String): Outcome<IntercomCall>
    suspend fun answer(callId: String): Outcome<IntercomCall>
    suspend fun hangup(callId: String, reason: IntercomEndReason = IntercomEndReason.UNSPECIFIED): Outcome<IntercomCall>
    suspend fun sendSignal(callId: String, signal: IntercomOutgoingSignal): Outcome<Boolean>
    suspend fun getIceConfig(): Outcome<List<IntercomIceServer>>
    suspend fun getStatus(): Outcome<IntercomStatus>

    fun watch(): Flow<IntercomWatchEvent>
}

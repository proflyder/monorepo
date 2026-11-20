package dev.proflyder.home.core.model

enum class IntercomEndpointKind {
    UNSPECIFIED,
    PHONE,
    STATION,
}

enum class IntercomCallState {
    UNSPECIFIED,
    RINGING,
    CONNECTING,
    ACTIVE,
    ENDED,
}

enum class IntercomEndReason {
    UNSPECIFIED,
    HANGUP,
    DECLINED,
    BUSY,
    CANCELED,
    TIMEOUT,
    UNREACHABLE,
    ERROR,
}

enum class IntercomSdpType {
    UNSPECIFIED,
    OFFER,
    ANSWER,
}

data class IntercomEndpoint(
    val id: String,
    val kind: IntercomEndpointKind,
    val displayName: String,
    val capabilities: List<String> = emptyList(),
    val online: Boolean = false,
    val lastSeenUnix: Long = 0,
)

data class IntercomCall(
    val id: String,
    val caller: IntercomEndpoint? = null,
    val callee: IntercomEndpoint? = null,
    val state: IntercomCallState = IntercomCallState.UNSPECIFIED,
    val endReason: IntercomEndReason = IntercomEndReason.UNSPECIFIED,
    val createdUnix: Long = 0,
    val updatedUnix: Long = 0,
    val answeredUnix: Long = 0,
    val endedUnix: Long = 0,
) {
    val isLive: Boolean get() = state != IntercomCallState.ENDED && state != IntercomCallState.UNSPECIFIED
}

data class IntercomSdpSignal(
    val type: IntercomSdpType,
    val sdp: String,
)

data class IntercomIceCandidate(
    val candidate: String,
    val sdpMid: String = "",
    val sdpMlineIndex: Int = 0,
)

data class IntercomIceServer(
    val urls: List<String> = emptyList(),
    val username: String = "",
    val credential: String = "",
)

data class IntercomStatus(
    val version: String = "",
    val nowUnix: Long = 0,
    val endpointsRegistered: Int = 0,
    val endpointsOnline: Int = 0,
    val activeCalls: Int = 0,
    val ready: Boolean = false,
    val ringTimeoutSeconds: Int = 0,
    val connectTimeoutSeconds: Int = 0,
)

sealed interface IntercomOutgoingSignal {
    data class Sdp(val signal: IntercomSdpSignal) : IntercomOutgoingSignal
    data class IceCandidate(val candidate: IntercomIceCandidate) : IntercomOutgoingSignal
    data object IceEnd : IntercomOutgoingSignal
}

sealed interface IntercomWatchEvent {
    data class CallChanged(
        val call: IntercomCall,
        val previousState: IntercomCallState,
    ) : IntercomWatchEvent

    data class Sdp(
        val callId: String,
        val fromEndpointId: String,
        val signal: IntercomSdpSignal,
    ) : IntercomWatchEvent

    data class IceCandidate(
        val callId: String,
        val fromEndpointId: String,
        val candidate: IntercomIceCandidate,
    ) : IntercomWatchEvent

    data class IceEnd(
        val callId: String,
        val fromEndpointId: String,
    ) : IntercomWatchEvent
}

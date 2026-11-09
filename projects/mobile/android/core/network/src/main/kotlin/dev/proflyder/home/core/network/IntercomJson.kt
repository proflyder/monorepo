package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.IntercomCall
import dev.proflyder.home.core.model.IntercomCallState
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomEndpointKind
import dev.proflyder.home.core.model.IntercomIceCandidate
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomSdpSignal
import dev.proflyder.home.core.model.IntercomSdpType
import dev.proflyder.home.core.model.IntercomStatus
import dev.proflyder.home.core.model.IntercomWatchEvent
import kotlinx.serialization.json.JsonElement

/**
 * Maps home.v1.IntercomService DTOs to domain models and builds typed requests.
 *
 * Enum values are the proto names and `int64` fields are decimal strings; both
 * are decoded by [WireDtos]. Unknown fields are ignored so a newer server does
 * not break an older client.
 */
object IntercomJson {

    fun parseEndpoint(element: JsonElement): IntercomEndpoint =
        WireJson.decodeFromJsonElement(EndpointDto.serializer(), element).toDomain()

    fun parseCall(element: JsonElement): IntercomCall =
        WireJson.decodeFromJsonElement(CallDto.serializer(), element).toDomain()

    fun parseWatchEvent(element: JsonElement): IntercomWatchEvent? =
        WireJson.decodeFromJsonElement(WatchEventDto.serializer(), element).toDomain()

    fun parseIceServers(element: JsonElement): List<IntercomIceServer> =
        WireJson.decodeFromJsonElement(IceConfigResponseDto.serializer(), element)
            .iceServers.map { it.toDomain() }

    fun parseStatus(element: JsonElement): IntercomStatus =
        WireJson.decodeFromJsonElement(IntercomStatusDto.serializer(), element).toDomain()

    fun registerEndpointRequest(displayName: String, capabilities: List<String>): RegisterEndpointRequestDto =
        RegisterEndpointRequestDto(EndpointInputDto(displayName = displayName, capabilities = capabilities))

    fun startCallRequest(calleeEndpointId: String, idempotencyKey: String): StartCallRequestDto =
        StartCallRequestDto(
            calleeEndpointId = calleeEndpointId,
            idempotencyKey = idempotencyKey.takeIf { it.isNotBlank() },
        )

    fun hangupRequest(callId: String, reason: IntercomEndReason): HangupRequestDto =
        HangupRequestDto(callId = callId, reason = reason.dto().takeIf { reason != IntercomEndReason.UNSPECIFIED })

    fun sendSignalRequest(callId: String, signal: IntercomOutgoingSignal): SendSignalRequestDto =
        SendSignalRequestDto(
            callId = callId,
            sdp = (signal as? IntercomOutgoingSignal.Sdp)?.signal?.toDto(),
            iceCandidate = (signal as? IntercomOutgoingSignal.IceCandidate)?.candidate?.toDto(),
            iceEnd = EmptyBody.takeIf { signal is IntercomOutgoingSignal.IceEnd },
        )
}

internal fun EndpointDto.toDomain(): IntercomEndpoint = IntercomEndpoint(
    id = id,
    kind = kind.toDomain(),
    displayName = displayName,
    capabilities = capabilities,
    online = online,
    lastSeenUnix = lastSeenUnix,
)

internal fun CallDto.toDomain(): IntercomCall = IntercomCall(
    id = id,
    caller = caller?.toDomain(),
    callee = callee?.toDomain(),
    state = state.toDomain(),
    endReason = endReason.toDomain(),
    createdUnix = createdUnix,
    updatedUnix = updatedUnix,
    answeredUnix = answeredUnix,
    endedUnix = endedUnix,
)

internal fun SdpSignalDto.toDomain(): IntercomSdpSignal =
    IntercomSdpSignal(type = type.toDomain(), sdp = sdp)

internal fun IceCandidateDto.toDomain(): IntercomIceCandidate =
    IntercomIceCandidate(candidate = candidate, sdpMid = sdpMid, sdpMlineIndex = sdpMlineIndex)

internal fun IceServerDto.toDomain(): IntercomIceServer =
    IntercomIceServer(urls = urls, username = username, credential = credential)

internal fun IntercomStatusDto.toDomain(): IntercomStatus = IntercomStatus(
    version = version,
    nowUnix = nowUnix,
    endpointsRegistered = endpointsRegistered,
    endpointsOnline = endpointsOnline,
    activeCalls = activeCalls,
    ready = ready,
    ringTimeoutSeconds = ringTimeoutSeconds,
    connectTimeoutSeconds = connectTimeoutSeconds,
)

internal fun WatchEventDto.toDomain(): IntercomWatchEvent? = when {
    call != null && call.call != null -> IntercomWatchEvent.CallChanged(
        call = call.call.toDomain(),
        previousState = call.previousState.toDomain(),
    )
    sdp != null && sdp.signal != null -> IntercomWatchEvent.Sdp(
        callId = sdp.callId,
        fromEndpointId = sdp.fromEndpointId,
        signal = sdp.signal.toDomain(),
    )
    iceCandidate != null && iceCandidate.candidate != null -> IntercomWatchEvent.IceCandidate(
        callId = iceCandidate.callId,
        fromEndpointId = iceCandidate.fromEndpointId,
        candidate = iceCandidate.candidate.toDomain(),
    )
    iceEnd != null -> IntercomWatchEvent.IceEnd(
        callId = iceEnd.callId,
        fromEndpointId = iceEnd.fromEndpointId,
    )
    else -> null
}

internal fun IntercomSdpSignal.toDto(): OutgoingSdpDto =
    OutgoingSdpDto(type = type.dto(), sdp = sdp)

internal fun IntercomIceCandidate.toDto(): IceCandidateDto =
    IceCandidateDto(candidate = candidate, sdpMid = sdpMid, sdpMlineIndex = sdpMlineIndex)

internal fun IntercomEndpointKindDto.toDomain(): IntercomEndpointKind = when (this) {
    IntercomEndpointKindDto.UNSPECIFIED -> IntercomEndpointKind.UNSPECIFIED
    IntercomEndpointKindDto.PHONE -> IntercomEndpointKind.PHONE
    IntercomEndpointKindDto.STATION -> IntercomEndpointKind.STATION
}

internal fun IntercomCallStateDto.toDomain(): IntercomCallState = when (this) {
    IntercomCallStateDto.UNSPECIFIED -> IntercomCallState.UNSPECIFIED
    IntercomCallStateDto.RINGING -> IntercomCallState.RINGING
    IntercomCallStateDto.CONNECTING -> IntercomCallState.CONNECTING
    IntercomCallStateDto.ACTIVE -> IntercomCallState.ACTIVE
    IntercomCallStateDto.ENDED -> IntercomCallState.ENDED
}

internal fun IntercomEndReasonDto.toDomain(): IntercomEndReason = when (this) {
    IntercomEndReasonDto.UNSPECIFIED -> IntercomEndReason.UNSPECIFIED
    IntercomEndReasonDto.HANGUP -> IntercomEndReason.HANGUP
    IntercomEndReasonDto.DECLINED -> IntercomEndReason.DECLINED
    IntercomEndReasonDto.BUSY -> IntercomEndReason.BUSY
    IntercomEndReasonDto.CANCELED -> IntercomEndReason.CANCELED
    IntercomEndReasonDto.TIMEOUT -> IntercomEndReason.TIMEOUT
    IntercomEndReasonDto.UNREACHABLE -> IntercomEndReason.UNREACHABLE
    IntercomEndReasonDto.ERROR -> IntercomEndReason.ERROR
}

internal fun IntercomSdpTypeDto.toDomain(): IntercomSdpType = when (this) {
    IntercomSdpTypeDto.UNSPECIFIED -> IntercomSdpType.UNSPECIFIED
    IntercomSdpTypeDto.OFFER -> IntercomSdpType.OFFER
    IntercomSdpTypeDto.ANSWER -> IntercomSdpType.ANSWER
}

internal fun IntercomEndReason.dto(): IntercomEndReasonDto = when (this) {
    IntercomEndReason.UNSPECIFIED -> IntercomEndReasonDto.UNSPECIFIED
    IntercomEndReason.HANGUP -> IntercomEndReasonDto.HANGUP
    IntercomEndReason.DECLINED -> IntercomEndReasonDto.DECLINED
    IntercomEndReason.BUSY -> IntercomEndReasonDto.BUSY
    IntercomEndReason.CANCELED -> IntercomEndReasonDto.CANCELED
    IntercomEndReason.TIMEOUT -> IntercomEndReasonDto.TIMEOUT
    IntercomEndReason.UNREACHABLE -> IntercomEndReasonDto.UNREACHABLE
    IntercomEndReason.ERROR -> IntercomEndReasonDto.ERROR
}

internal fun IntercomSdpType.dto(): IntercomSdpTypeDto = when (this) {
    IntercomSdpType.UNSPECIFIED -> IntercomSdpTypeDto.UNSPECIFIED
    IntercomSdpType.OFFER -> IntercomSdpTypeDto.OFFER
    IntercomSdpType.ANSWER -> IntercomSdpTypeDto.ANSWER
}

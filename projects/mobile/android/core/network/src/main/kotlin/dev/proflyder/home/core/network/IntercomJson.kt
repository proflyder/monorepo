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
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull

/**
 * Protobuf-JSON mapping for home.v1.IntercomService.
 *
 * Enum values are the proto names (`CALL_STATE_ACTIVE`); `int64` fields are
 * decimal strings (a plain JSON number is also tolerated). Unknown fields are
 * ignored so a newer server does not break an older client.
 */
object IntercomJson {

    fun parseEndpoint(element: JsonElement): IntercomEndpoint = element.jsonObject.let { obj ->
        IntercomEndpoint(
            id = obj.stringOrEmpty("id"),
            kind = when (obj.stringOrEmpty("kind")) {
                "ENDPOINT_KIND_PHONE" -> IntercomEndpointKind.PHONE
                "ENDPOINT_KIND_STATION" -> IntercomEndpointKind.STATION
                else -> IntercomEndpointKind.UNSPECIFIED
            },
            displayName = obj.stringOrEmpty("displayName"),
            capabilities = (obj["capabilities"] as? JsonArray)?.mapNotNull {
                (it as? JsonPrimitive)?.contentOrNull
            }.orEmpty(),
            online = obj["online"]?.jsonPrimitive?.booleanOrNull ?: false,
            lastSeenUnix = obj.longOrZero("lastSeenUnix"),
        )
    }

    fun parseCall(element: JsonElement): IntercomCall = element.jsonObject.let { obj ->
        IntercomCall(
            id = obj.stringOrEmpty("id"),
            caller = obj["caller"]?.let(::parseEndpoint),
            callee = obj["callee"]?.let(::parseEndpoint),
            state = when (obj.stringOrEmpty("state")) {
                "CALL_STATE_RINGING" -> IntercomCallState.RINGING
                "CALL_STATE_CONNECTING" -> IntercomCallState.CONNECTING
                "CALL_STATE_ACTIVE" -> IntercomCallState.ACTIVE
                "CALL_STATE_ENDED" -> IntercomCallState.ENDED
                else -> IntercomCallState.UNSPECIFIED
            },
            endReason = when (obj.stringOrEmpty("endReason")) {
                "END_REASON_HANGUP" -> IntercomEndReason.HANGUP
                "END_REASON_DECLINED" -> IntercomEndReason.DECLINED
                "END_REASON_BUSY" -> IntercomEndReason.BUSY
                "END_REASON_CANCELED" -> IntercomEndReason.CANCELED
                "END_REASON_TIMEOUT" -> IntercomEndReason.TIMEOUT
                "END_REASON_UNREACHABLE" -> IntercomEndReason.UNREACHABLE
                "END_REASON_ERROR" -> IntercomEndReason.ERROR
                else -> IntercomEndReason.UNSPECIFIED
            },
            createdUnix = obj.longOrZero("createdUnix"),
            updatedUnix = obj.longOrZero("updatedUnix"),
            answeredUnix = obj.longOrZero("answeredUnix"),
            endedUnix = obj.longOrZero("endedUnix"),
        )
    }

    fun parseWatchEvent(element: JsonElement): IntercomWatchEvent? {
        val obj = element.jsonObject
        obj["call"]?.let { callEvent ->
            val event = callEvent.jsonObject
            val call = event["call"] ?: return null
            return IntercomWatchEvent.CallChanged(
                call = parseCall(call),
                previousState = parseState(event.stringOrEmpty("previousState")),
            )
        }
        obj["sdp"]?.let { sdpEvent ->
            val event = sdpEvent.jsonObject
            val signal = event["signal"]?.jsonObject ?: return null
            return IntercomWatchEvent.Sdp(
                callId = event.stringOrEmpty("callId"),
                fromEndpointId = event.stringOrEmpty("fromEndpointId"),
                signal = IntercomSdpSignal(
                    type = when (signal.stringOrEmpty("type")) {
                        "SDP_TYPE_OFFER" -> IntercomSdpType.OFFER
                        "SDP_TYPE_ANSWER" -> IntercomSdpType.ANSWER
                        else -> IntercomSdpType.UNSPECIFIED
                    },
                    sdp = signal.stringOrEmpty("sdp"),
                ),
            )
        }
        obj["iceCandidate"]?.let { iceEvent ->
            val event = iceEvent.jsonObject
            val candidate = event["candidate"]?.jsonObject ?: return null
            return IntercomWatchEvent.IceCandidate(
                callId = event.stringOrEmpty("callId"),
                fromEndpointId = event.stringOrEmpty("fromEndpointId"),
                candidate = IntercomIceCandidate(
                    candidate = candidate.stringOrEmpty("candidate"),
                    sdpMid = candidate.stringOrEmpty("sdpMid"),
                    sdpMlineIndex = candidate["sdpMlineIndex"]?.jsonPrimitive?.intOrNull ?: 0,
                ),
            )
        }
        obj["iceEnd"]?.let { iceEnd ->
            val event = iceEnd.jsonObject
            return IntercomWatchEvent.IceEnd(
                callId = event.stringOrEmpty("callId"),
                fromEndpointId = event.stringOrEmpty("fromEndpointId"),
            )
        }
        return null
    }

    fun parseIceServers(element: JsonElement): List<IntercomIceServer> {
        val array = element.jsonObject["iceServers"] as? JsonArray ?: return emptyList()
        return array.map { item ->
            val obj = item.jsonObject
            IntercomIceServer(
                urls = (obj["urls"] as? JsonArray)?.mapNotNull {
                    (it as? JsonPrimitive)?.contentOrNull
                }.orEmpty(),
                username = obj.stringOrEmpty("username"),
                credential = obj.stringOrEmpty("credential"),
            )
        }
    }

    fun parseStatus(element: JsonElement): IntercomStatus = element.jsonObject.let { obj ->
        IntercomStatus(
            version = obj.stringOrEmpty("version"),
            nowUnix = obj.longOrZero("nowUnix"),
            endpointsRegistered = obj["endpointsRegistered"]?.jsonPrimitive?.intOrNull ?: 0,
            endpointsOnline = obj["endpointsOnline"]?.jsonPrimitive?.intOrNull ?: 0,
            activeCalls = obj["activeCalls"]?.jsonPrimitive?.intOrNull ?: 0,
            ready = obj["ready"]?.jsonPrimitive?.booleanOrNull ?: false,
            ringTimeoutSeconds = obj["ringTimeoutSeconds"]?.jsonPrimitive?.intOrNull ?: 0,
            connectTimeoutSeconds = obj["connectTimeoutSeconds"]?.jsonPrimitive?.intOrNull ?: 0,
        )
    }

    fun registerEndpointRequest(displayName: String, capabilities: List<String>): JsonObject =
        buildJsonObject {
            put(
                "endpoint",
                buildJsonObject {
                    put("displayName", displayName)
                    put("capabilities", buildJsonArray { capabilities.forEach { add(JsonPrimitive(it)) } })
                },
            )
        }

    fun startCallRequest(calleeEndpointId: String, idempotencyKey: String): JsonObject =
        buildJsonObject {
            put("calleeEndpointId", calleeEndpointId)
            if (idempotencyKey.isNotBlank()) put("idempotencyKey", idempotencyKey)
        }

    fun hangupRequest(callId: String, reason: IntercomEndReason): JsonObject =
        buildJsonObject {
            put("callId", callId)
            if (reason != IntercomEndReason.UNSPECIFIED) {
                put("reason", "END_REASON_${reason.name}")
            }
        }

    fun sendSignalRequest(callId: String, signal: IntercomOutgoingSignal): JsonObject =
        buildJsonObject {
            put("callId", callId)
            when (signal) {
                is IntercomOutgoingSignal.Sdp -> put(
                    "sdp",
                    buildJsonObject {
                        put("type", "SDP_TYPE_${signal.signal.type.name}")
                        put("sdp", signal.signal.sdp)
                    },
                )
                is IntercomOutgoingSignal.IceCandidate -> put(
                    "iceCandidate",
                    buildJsonObject {
                        put("candidate", signal.candidate.candidate)
                        put("sdpMid", signal.candidate.sdpMid)
                        put("sdpMlineIndex", signal.candidate.sdpMlineIndex)
                    },
                )
                IntercomOutgoingSignal.IceEnd -> put("iceEnd", buildJsonObject { })
            }
        }

    private fun parseState(raw: String): IntercomCallState = when (raw) {
        "CALL_STATE_RINGING" -> IntercomCallState.RINGING
        "CALL_STATE_CONNECTING" -> IntercomCallState.CONNECTING
        "CALL_STATE_ACTIVE" -> IntercomCallState.ACTIVE
        "CALL_STATE_ENDED" -> IntercomCallState.ENDED
        else -> IntercomCallState.UNSPECIFIED
    }
}

internal fun JsonObject.stringOrEmpty(key: String): String =
    (this[key] as? JsonPrimitive)?.contentOrNull.orEmpty()

internal fun JsonObject.longOrZero(key: String): Long =
    (this[key] as? JsonPrimitive)?.let { it.longOrNull ?: it.contentOrNull?.toLongOrNull() } ?: 0L

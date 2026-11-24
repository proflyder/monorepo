package dev.proflyder.home.core.network

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull

internal object Int64StringSerializer : KSerializer<Long> {
    override val descriptor =
        PrimitiveSerialDescriptor("dev.proflyder.home.Int64String", PrimitiveKind.STRING)

    override fun deserialize(decoder: Decoder): Long {
        val input = decoder as? JsonDecoder ?: return decoder.decodeLong()
        val element = input.decodeJsonElement()
        val primitive = element as? JsonPrimitive ?: return 0L
        return primitive.longOrNull ?: primitive.contentOrNull?.toLongOrNull() ?: 0L
    }

    override fun serialize(encoder: Encoder, value: Long) {
        if (encoder is JsonEncoder) encoder.encodeJsonElement(JsonPrimitive(value.toString()))
        else encoder.encodeLong(value)
    }
}

@Suppress("LongLambda")
internal val NetworkJson = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
    explicitNulls = false
    encodeDefaults = true
}

@Serializable
internal data object EmptyBody

@Serializable
internal data class ConnectErrorDto(
    val code: String = "",
    val message: String = "",
    val details: List<ConnectDetailDto> = emptyList(),
)

@Serializable
internal data class ConnectDetailDto(
    val type: String = "",
    val debug: LocalizedDebugDto? = null,
)

@Serializable
internal data class LocalizedDebugDto(
    val key: String = "",
    val args: Map<String, String> = emptyMap(),
    val module: String = "",
)

@Serializable
internal data class WallTimeDto(
    val hour: Int = 0,
    val minute: Int = 0,
    val second: Int = 0,
)

@Serializable
internal enum class ScheduleKindDto {
    @SerialName("SCHEDULE_KIND_UNSPECIFIED") UNSPECIFIED,
    @SerialName("SCHEDULE_KIND_ALARM") ALARM,
    @SerialName("SCHEDULE_KIND_REMINDER") REMINDER,
    @SerialName("SCHEDULE_KIND_SCENARIO") SCENARIO,
}

@Serializable
internal enum class RepeatKindDto {
    @SerialName("REPEAT_KIND_UNSPECIFIED") UNSPECIFIED,
    @SerialName("REPEAT_KIND_ONCE") ONCE,
    @SerialName("REPEAT_KIND_DAILY") DAILY,
    @SerialName("REPEAT_KIND_WEEKDAYS") WEEKDAYS,
    @SerialName("REPEAT_KIND_WEEKENDS") WEEKENDS,
    @SerialName("REPEAT_KIND_CUSTOM") CUSTOM,
}

@Serializable
internal enum class TimerStateDto {
    @SerialName("TIMER_STATE_UNSPECIFIED") UNSPECIFIED,
    @SerialName("TIMER_STATE_RUNNING") RUNNING,
    @SerialName("TIMER_STATE_PAUSED") PAUSED,
    @SerialName("TIMER_STATE_FIRED") FIRED,
    @SerialName("TIMER_STATE_CANCELLED") CANCELLED,
}

@Serializable
internal data class ScheduleDto(
    val id: String = "",
    val label: String = "",
    val kind: ScheduleKindDto = ScheduleKindDto.UNSPECIFIED,
    val at: WallTimeDto? = null,
    val onceDate: String = "",
    val repeat: RepeatKindDto = RepeatKindDto.UNSPECIFIED,
    val weekdays: List<Int> = emptyList(),
    @Serializable(with = Int64StringSerializer::class) val nextDueUnix: Long = 0,
    val enabled: Boolean = false,
    @Serializable(with = Int64StringSerializer::class) val snoozeUntilUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val lastFiredUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val snoozeCount: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val revision: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val createdUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val updatedUnix: Long = 0,
    val payloadJson: String = "",
    @Serializable(with = Int64StringSerializer::class) val activeOccurrenceUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val activeUntilUnix: Long = 0,
)

@Serializable
internal data class ScheduleInputDto(
    val id: String? = null,
    val label: String,
    val kind: ScheduleKindDto,
    val at: WallTimeDto,
    val onceDate: String? = null,
    val repeat: RepeatKindDto,
    val weekdays: List<Int>? = null,
    val enabled: Boolean,
    val payloadJson: String? = null,
    @Serializable(with = Int64StringSerializer::class) val revision: Long? = null,
)

@Serializable
internal data class TimerDto(
    val id: String = "",
    val label: String = "",
    @Serializable(with = Int64StringSerializer::class) val durationSeconds: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val remainingSeconds: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val dueUnix: Long = 0,
    val state: TimerStateDto = TimerStateDto.UNSPECIFIED,
    @Serializable(with = Int64StringSerializer::class) val revision: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val createdUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val updatedUnix: Long = 0,
)

@Serializable
internal data class ConsumerStatusDto(
    val consumerId: String = "",
    @Serializable(with = Int64StringSerializer::class) val lastSeenUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val lastAckUnix: Long = 0,
    val alive: Boolean = false,
)

@Serializable
internal data class SchedulerStatusDto(
    val version: String = "",
    val schemaVersion: Int = 0,
    @Serializable(with = Int64StringSerializer::class) val nowUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val lastTickUnix: Long = 0,
    val pendingEvents: Int = 0,
    val leasedEvents: Int = 0,
    val enabledSchedules: Int = 0,
    val runningTimers: Int = 0,
    val consumers: List<ConsumerStatusDto> = emptyList(),
    val scenarioExecutionSupported: Boolean = false,
)

@Serializable
internal data class EventDto(val id: String = "")

@Serializable
internal data class DeleteResponseDto(val deleted: Boolean = false)

@Serializable
internal data class SchedulerStatusResponseDto(val status: SchedulerStatusDto = SchedulerStatusDto())

@Serializable
internal data class ScheduleResponseDto(val schedule: ScheduleDto)

@Serializable
internal data class SchedulesResponseDto(val schedules: List<ScheduleDto>? = emptyList())

@Serializable
internal data class TimerResponseDto(val timer: TimerDto)

@Serializable
internal data class TimersResponseDto(val timers: List<TimerDto>? = emptyList())

@Serializable
internal data class EventResponseDto(val event: EventDto)

@Serializable
internal data class ListSchedulesRequestDto(
    val includeDisabled: Boolean,
    val kind: ScheduleKindDto? = null,
)

@Serializable
internal data class ScheduleIdRequestDto(val id: String)

@Serializable
internal data class SetScheduleEnabledRequestDto(val id: String, val enabled: Boolean)

@Serializable
internal data class SnoozeScheduleRequestDto(val id: String, val minutes: Int)

@Serializable
internal data class CreateScheduleRequestDto(
    val label: String,
    val kind: ScheduleKindDto,
    val at: WallTimeDto,
    val onceDate: String? = null,
    val repeat: RepeatKindDto,
    val weekdays: List<Int>? = null,
    val enabled: Boolean,
    val payloadJson: String? = null,
    val idempotencyKey: String? = null,
)

@Serializable
internal data class UpdateScheduleRequestDto(
    val schedule: ScheduleInputDto,
    @Serializable(with = Int64StringSerializer::class) val expectedRevision: Long,
)

@Serializable
internal data class StartTimerRequestDto(
    val label: String,
    @Serializable(with = Int64StringSerializer::class) val durationSeconds: Long,
    val idempotencyKey: String? = null,
)

@Serializable
internal data class ListTimersRequestDto(val includeFinished: Boolean)

@Serializable
internal data class EnqueueTaskRequestDto(
    val taskType: String,
    val payloadJson: String,
    val queue: String,
    val idempotencyKey: String? = null,
)

@Serializable
internal data class ScenarioDto(
    val id: String = "",
    val label: String = "",
    val description: String = "",
    val enabled: Boolean = false,
    @Serializable(with = Int64StringSerializer::class) val revision: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val createdUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val updatedUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val nextDueUnix: Long = 0,
    val triggers: List<JsonElement> = emptyList(),
    val conditions: List<JsonElement> = emptyList(),
    val steps: List<JsonElement> = emptyList(),
    val continueOnError: Boolean = false,
)

@Serializable
internal data class TimeTriggerDto(
    val timezone: String = "",
    val at: WallTimeDto? = null,
    val repeat: RepeatKindDto = RepeatKindDto.UNSPECIFIED,
    val onceDate: String = "",
    val weekdays: List<Int> = emptyList(),
)

@Serializable
internal data class EntityStateConditionDto(
    val entityId: String = "",
    val state: String = "",
)

@Serializable
internal data class TimeWindowConditionDto(
    val start: WallTimeDto? = null,
    val end: WallTimeDto? = null,
    val weekdays: List<Int> = emptyList(),
)

@Serializable
internal data class CatalogStepDto(
    val entryId: String = "",
    val action: String = "",
    val paramsJson: String = "",
)

@Serializable
internal data class SpeakStepDto(val text: String = "")

@Serializable
internal data class DelayStepDto(val seconds: Int = 0)

@Serializable
internal data class ScenarioInputDto(
    val id: String? = null,
    val label: String,
    val description: String = "",
    val enabled: Boolean,
    val continueOnError: Boolean,
    val triggers: List<JsonElement> = emptyList(),
    val conditions: List<JsonElement> = emptyList(),
    val steps: List<JsonElement> = emptyList(),
    @Serializable(with = Int64StringSerializer::class) val revision: Long? = null,
)

@Serializable
internal data class ScenarioResponseDto(val scenario: ScenarioDto)

@Serializable
internal data class ScenariosResponseDto(val scenarios: List<ScenarioDto>? = emptyList())

@Serializable
internal data class ListScenariosRequestDto(val includeDisabled: Boolean)

@Serializable
internal data class ScenarioIdRequestDto(val id: String)

@Serializable
internal data class CreateScenarioRequestDto(
    val label: String,
    val description: String = "",
    val enabled: Boolean,
    val continueOnError: Boolean,
    val triggers: List<JsonElement> = emptyList(),
    val conditions: List<JsonElement> = emptyList(),
    val steps: List<JsonElement> = emptyList(),
    val idempotencyKey: String? = null,
)

@Serializable
internal data class UpdateScenarioRequestDto(
    val scenario: ScenarioInputDto,
    @Serializable(with = Int64StringSerializer::class) val expectedRevision: Long,
)

@Serializable
internal data class SetScenarioEnabledRequestDto(val id: String, val enabled: Boolean)

@Serializable
internal data class RunScenarioRequestDto(val id: String, val idempotencyKey: String? = null)

@Serializable
internal enum class IntercomEndpointKindDto {
    @SerialName("ENDPOINT_KIND_UNSPECIFIED") UNSPECIFIED,
    @SerialName("ENDPOINT_KIND_PHONE") PHONE,
    @SerialName("ENDPOINT_KIND_STATION") STATION,
}

@Serializable
internal enum class IntercomCallStateDto {
    @SerialName("CALL_STATE_UNSPECIFIED") UNSPECIFIED,
    @SerialName("CALL_STATE_RINGING") RINGING,
    @SerialName("CALL_STATE_CONNECTING") CONNECTING,
    @SerialName("CALL_STATE_ACTIVE") ACTIVE,
    @SerialName("CALL_STATE_ENDED") ENDED,
}

@Serializable
internal enum class IntercomEndReasonDto {
    @SerialName("END_REASON_UNSPECIFIED") UNSPECIFIED,
    @SerialName("END_REASON_HANGUP") HANGUP,
    @SerialName("END_REASON_DECLINED") DECLINED,
    @SerialName("END_REASON_BUSY") BUSY,
    @SerialName("END_REASON_CANCELED") CANCELED,
    @SerialName("END_REASON_TIMEOUT") TIMEOUT,
    @SerialName("END_REASON_UNREACHABLE") UNREACHABLE,
    @SerialName("END_REASON_ERROR") ERROR,
}

@Serializable
internal enum class IntercomSdpTypeDto {
    @SerialName("SDP_TYPE_UNSPECIFIED") UNSPECIFIED,
    @SerialName("SDP_TYPE_OFFER") OFFER,
    @SerialName("SDP_TYPE_ANSWER") ANSWER,
}

@Serializable
internal data class EndpointDto(
    val id: String = "",
    val kind: IntercomEndpointKindDto = IntercomEndpointKindDto.UNSPECIFIED,
    val displayName: String = "",
    val capabilities: List<String> = emptyList(),
    val online: Boolean = false,
    @Serializable(with = Int64StringSerializer::class) val lastSeenUnix: Long = 0,
)

@Serializable
internal data class CallDto(
    val id: String = "",
    val caller: EndpointDto? = null,
    val callee: EndpointDto? = null,
    val state: IntercomCallStateDto = IntercomCallStateDto.UNSPECIFIED,
    val endReason: IntercomEndReasonDto = IntercomEndReasonDto.UNSPECIFIED,
    @Serializable(with = Int64StringSerializer::class) val createdUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val updatedUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val answeredUnix: Long = 0,
    @Serializable(with = Int64StringSerializer::class) val endedUnix: Long = 0,
)

@Serializable
internal data class SdpSignalDto(
    val type: IntercomSdpTypeDto = IntercomSdpTypeDto.UNSPECIFIED,
    val sdp: String = "",
)

@Serializable
internal data class IceCandidateDto(
    val candidate: String = "",
    val sdpMid: String = "",
    val sdpMlineIndex: Int = 0,
)

@Serializable
internal data class IceServerDto(
    val urls: List<String> = emptyList(),
    val username: String = "",
    val credential: String = "",
)

@Serializable
internal data class IntercomStatusDto(
    val version: String = "",
    @Serializable(with = Int64StringSerializer::class) val nowUnix: Long = 0,
    val endpointsRegistered: Int = 0,
    val endpointsOnline: Int = 0,
    val activeCalls: Int = 0,
    val ready: Boolean = false,
    val ringTimeoutSeconds: Int = 0,
    val connectTimeoutSeconds: Int = 0,
)

@Serializable
internal data class EndpointInputDto(
    val displayName: String,
    val capabilities: List<String> = emptyList(),
)

@Serializable
internal data class RegisterEndpointRequestDto(val endpoint: EndpointInputDto)

@Serializable
internal data class EndpointResponseDto(val endpoint: EndpointDto)

@Serializable
internal data class EndpointsResponseDto(val endpoints: List<EndpointDto> = emptyList())

@Serializable
internal data class StartCallRequestDto(
    val calleeEndpointId: String,
    val idempotencyKey: String? = null,
)

@Serializable
internal data class CallRequestDto(val callId: String)

@Serializable
internal data class HangupRequestDto(
    val callId: String,
    val reason: IntercomEndReasonDto? = null,
)

@Serializable
internal data class OutgoingSdpDto(
    val type: IntercomSdpTypeDto,
    val sdp: String,
)

@Serializable
internal data class SendSignalRequestDto(
    val callId: String,
    val sdp: OutgoingSdpDto? = null,
    val iceCandidate: IceCandidateDto? = null,
    val iceEnd: EmptyBody? = null,
)

@Serializable
internal data class CallResponseDto(val call: CallDto)

@Serializable
internal data class DeliveredResponseDto(val delivered: Boolean = false)

@Serializable
internal data class IceConfigResponseDto(val iceServers: List<IceServerDto> = emptyList())

@Serializable
internal data class CallChangedDto(
    val call: CallDto? = null,
    val previousState: IntercomCallStateDto = IntercomCallStateDto.UNSPECIFIED,
)

@Serializable
internal data class SdpEventDto(
    val callId: String = "",
    val fromEndpointId: String = "",
    val signal: SdpSignalDto? = null,
)

@Serializable
internal data class IceEventDto(
    val callId: String = "",
    val fromEndpointId: String = "",
    val candidate: IceCandidateDto? = null,
)

@Serializable
internal data class IceEndEventDto(
    val callId: String = "",
    val fromEndpointId: String = "",
)

@Serializable
internal data class WatchEventDto(
    val call: CallChangedDto? = null,
    val sdp: SdpEventDto? = null,
    val iceCandidate: IceEventDto? = null,
    val iceEnd: IceEndEventDto? = null,
)

@Serializable
internal data class ControlCatalogDto(val entries: List<ControlEntryDto>, val statesOk: Boolean = false)

@Serializable
internal data class ControlEntryDto(
    val id: String = "",
    val kind: String = "",
    val title: String = "",
    val titleKey: String = "",
    val titleArgs: Map<String, String> = emptyMap(),
    val detailKey: String = "",
    val room: String = "",
    val icon: String = "",
    val actions: List<String> = emptyList(),
    val state: String? = null,
    val available: Boolean = false,
    val light: LightControlsDto? = null,
    val volume: VolumeControlsDto? = null,
    @SerialName("device_id") val deviceIdSnake: String = "",
    val deviceId: String = "",
    val device: String = "",
    val role: String = "",
    val visibleWhen: VisibleWhenDto? = null,
)

@Serializable
internal data class VisibleWhenDto(
    val entry: String = "",
    val state: String = "",
)

@Serializable
internal data class VolumeControlsDto(
    val level: Int = 0,
    val muted: Boolean = false,
)

@Serializable
internal data class LightControlsDto(
    val brightness: Boolean = false,
    val colorTemp: Boolean = false,
    val color: Boolean = false,
    val minKelvin: Int = 0,
    val maxKelvin: Int = 0,
    val brightnessPct: Int = 0,
    val kelvin: Int = 0,
    val hue: Int = 0,
    val saturation: Int = 0,
    val mode: String = "",
    val effects: List<String> = emptyList(),
)

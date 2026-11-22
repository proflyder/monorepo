package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import java.lang.reflect.Type
import okhttp3.MediaType
import okhttp3.RequestBody
import okhttp3.ResponseBody
import okio.BufferedSink
import retrofit2.Converter
import retrofit2.Retrofit
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST

internal interface SchedulerApi {
    @Idempotent
    @POST("home.v1.SchedulerService/GetStatus")
    suspend fun getStatus(@Body body: EmptyBody): Outcome<SchedulerStatusDto>

    @Idempotent
    @POST("home.v1.SchedulerService/ListSchedules")
    suspend fun listSchedules(@Body body: ListSchedulesRequestDto): Outcome<SchedulesResponseDto>

    @Idempotent
    @POST("home.v1.SchedulerService/GetSchedule")
    suspend fun getSchedule(@Body body: ScheduleIdRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/CreateSchedule")
    suspend fun createSchedule(@Body body: CreateScheduleRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/UpdateSchedule")
    suspend fun updateSchedule(@Body body: UpdateScheduleRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/DeleteSchedule")
    suspend fun deleteSchedule(@Body body: ScheduleIdRequestDto): Outcome<DeleteResponseDto>

    @POST("home.v1.SchedulerService/SetScheduleEnabled")
    suspend fun setScheduleEnabled(@Body body: SetScheduleEnabledRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/SnoozeSchedule")
    suspend fun snoozeSchedule(@Body body: SnoozeScheduleRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/StopSchedule")
    suspend fun stopSchedule(@Body body: ScheduleIdRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/SkipScheduleOccurrence")
    suspend fun skipScheduleOccurrence(@Body body: ScheduleIdRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/StartTimer")
    suspend fun startTimer(@Body body: StartTimerRequestDto): Outcome<TimerResponseDto>

    @Idempotent
    @POST("home.v1.SchedulerService/ListTimers")
    suspend fun listTimers(@Body body: ListTimersRequestDto): Outcome<TimersResponseDto>

    @POST("home.v1.SchedulerService/PauseTimer")
    suspend fun pauseTimer(@Body body: ScheduleIdRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/ResumeTimer")
    suspend fun resumeTimer(@Body body: ScheduleIdRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/CancelTimer")
    suspend fun cancelTimer(@Body body: ScheduleIdRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/EnqueueTask")
    suspend fun enqueueTask(@Body body: EnqueueTaskRequestDto): Outcome<EventResponseDto>
}

internal interface ScenarioApi {
    @Idempotent
    @POST("home.v1.ScenarioService/ListScenarios")
    suspend fun listScenarios(@Body body: ListScenariosRequestDto): Outcome<ScenariosResponseDto>

    @Idempotent
    @POST("home.v1.ScenarioService/GetScenario")
    suspend fun getScenario(@Body body: ScenarioIdRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/CreateScenario")
    suspend fun createScenario(@Body body: CreateScenarioRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/UpdateScenario")
    suspend fun updateScenario(@Body body: UpdateScenarioRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/DeleteScenario")
    suspend fun deleteScenario(@Body body: ScenarioIdRequestDto): Outcome<DeleteResponseDto>

    @POST("home.v1.ScenarioService/SetScenarioEnabled")
    suspend fun setScenarioEnabled(@Body body: SetScenarioEnabledRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/RunScenario")
    suspend fun runScenario(@Body body: RunScenarioRequestDto): Outcome<EventResponseDto>
}

internal interface IntercomApi {
    @POST("home.v1.IntercomService/RegisterEndpoint")
    suspend fun registerEndpoint(@Body body: RegisterEndpointRequestDto): Outcome<EndpointResponseDto>

    @Idempotent
    @POST("home.v1.IntercomService/ListEndpoints")
    suspend fun listEndpoints(@Body body: EmptyBody): Outcome<EndpointsResponseDto>

    @POST("home.v1.IntercomService/StartCall")
    suspend fun startCall(@Body body: StartCallRequestDto): Outcome<CallResponseDto>

    @POST("home.v1.IntercomService/Answer")
    suspend fun answer(@Body body: CallRequestDto): Outcome<CallResponseDto>

    @POST("home.v1.IntercomService/Hangup")
    suspend fun hangup(@Body body: HangupRequestDto): Outcome<CallResponseDto>

    @POST("home.v1.IntercomService/SendSignal")
    suspend fun sendSignal(@Body body: SendSignalRequestDto): Outcome<DeliveredResponseDto>

    @Idempotent
    @POST("home.v1.IntercomService/GetIceConfig")
    suspend fun getIceConfig(@Body body: EmptyBody): Outcome<IceConfigResponseDto>

    @Idempotent
    @POST("home.v1.IntercomService/GetStatus")
    suspend fun getStatus(@Body body: EmptyBody): Outcome<IntercomStatusDto>
}

internal interface ControlApi {
    @Idempotent
    @GET("v1/catalog")
    suspend fun catalog(): Outcome<ControlCatalogDto>
}

internal class BoundedResponseConverterFactory(
    private val delegate: Converter.Factory,
    private val contentType: MediaType,
) : Converter.Factory() {
    override fun requestBodyConverter(
        type: Type,
        parameterAnnotations: Array<out Annotation>,
        methodAnnotations: Array<out Annotation>,
        retrofit: Retrofit,
    ): Converter<*, RequestBody>? {
        val converter = delegate.requestBodyConverter(
            type,
            parameterAnnotations,
            methodAnnotations,
            retrofit,
        ) ?: return null
        @Suppress("UNCHECKED_CAST")
        val typed = converter as Converter<Any, RequestBody>
        return Converter<Any, RequestBody> { value ->
            PlainJsonBody(requireNotNull(typed.convert(value)), contentType)
        }
    }

    override fun responseBodyConverter(
        type: Type,
        annotations: Array<out Annotation>,
        retrofit: Retrofit,
    ): Converter<ResponseBody, *>? {
        val converter = delegate.responseBodyConverter(type, annotations, retrofit) ?: return null
        return Converter<ResponseBody, Any?> { body ->
            body.use {
                it.requireWithinLimit()
                converter.convert(it)
            }
        }
    }
}

private class PlainJsonBody(
    private val delegate: RequestBody,
    private val contentType: MediaType,
) : RequestBody() {
    override fun contentType(): MediaType = contentType

    override fun contentLength(): Long = delegate.contentLength()

    override fun writeTo(sink: BufferedSink) = delegate.writeTo(sink)
}

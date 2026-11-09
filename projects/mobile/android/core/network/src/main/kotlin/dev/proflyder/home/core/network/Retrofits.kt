package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import java.lang.reflect.Type
import okhttp3.Interceptor
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.RequestBody
import okhttp3.ResponseBody
import okio.BufferedSink
import retrofit2.Converter
import retrofit2.Retrofit
import retrofit2.converter.kotlinx.serialization.asConverterFactory
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST

/** Connect unary calls of home.v1.SchedulerService. */
internal interface SchedulerApi {
    @POST("home.v1.SchedulerService/GetStatus")
    fun getStatus(@Body body: EmptyBody): Outcome<SchedulerStatusDto>

    @POST("home.v1.SchedulerService/ListSchedules")
    fun listSchedules(@Body body: ListSchedulesRequestDto): Outcome<SchedulesResponseDto>

    @POST("home.v1.SchedulerService/GetSchedule")
    fun getSchedule(@Body body: ScheduleIdRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/CreateSchedule")
    fun createSchedule(@Body body: CreateScheduleRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/UpdateSchedule")
    fun updateSchedule(@Body body: UpdateScheduleRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/DeleteSchedule")
    fun deleteSchedule(@Body body: ScheduleIdRequestDto): Outcome<DeleteResponseDto>

    @POST("home.v1.SchedulerService/SetScheduleEnabled")
    fun setScheduleEnabled(@Body body: SetScheduleEnabledRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/SnoozeSchedule")
    fun snoozeSchedule(@Body body: SnoozeScheduleRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/StopSchedule")
    fun stopSchedule(@Body body: ScheduleIdRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/SkipScheduleOccurrence")
    fun skipScheduleOccurrence(@Body body: ScheduleIdRequestDto): Outcome<ScheduleResponseDto>

    @POST("home.v1.SchedulerService/StartTimer")
    fun startTimer(@Body body: StartTimerRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/ListTimers")
    fun listTimers(@Body body: ListTimersRequestDto): Outcome<TimersResponseDto>

    @POST("home.v1.SchedulerService/PauseTimer")
    fun pauseTimer(@Body body: ScheduleIdRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/ResumeTimer")
    fun resumeTimer(@Body body: ScheduleIdRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/CancelTimer")
    fun cancelTimer(@Body body: ScheduleIdRequestDto): Outcome<TimerResponseDto>

    @POST("home.v1.SchedulerService/EnqueueTask")
    fun enqueueTask(@Body body: EnqueueTaskRequestDto): Outcome<EventResponseDto>
}

/** Connect unary calls of home.v1.ScenarioService on the same scheduler endpoint. */
internal interface ScenarioApi {
    @POST("home.v1.ScenarioService/ListScenarios")
    fun listScenarios(@Body body: ListScenariosRequestDto): Outcome<ScenariosResponseDto>

    @POST("home.v1.ScenarioService/GetScenario")
    fun getScenario(@Body body: ScenarioIdRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/CreateScenario")
    fun createScenario(@Body body: CreateScenarioRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/UpdateScenario")
    fun updateScenario(@Body body: UpdateScenarioRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/DeleteScenario")
    fun deleteScenario(@Body body: ScenarioIdRequestDto): Outcome<DeleteResponseDto>

    @POST("home.v1.ScenarioService/SetScenarioEnabled")
    fun setScenarioEnabled(@Body body: SetScenarioEnabledRequestDto): Outcome<ScenarioResponseDto>

    @POST("home.v1.ScenarioService/RunScenario")
    fun runScenario(@Body body: RunScenarioRequestDto): Outcome<EventResponseDto>
}

/** Connect unary calls of home.v1.IntercomService (signaling). */
internal interface IntercomApi {
    @POST("home.v1.IntercomService/RegisterEndpoint")
    fun registerEndpoint(@Body body: RegisterEndpointRequestDto): Outcome<EndpointResponseDto>

    @POST("home.v1.IntercomService/ListEndpoints")
    fun listEndpoints(@Body body: EmptyBody): Outcome<EndpointsResponseDto>

    @POST("home.v1.IntercomService/StartCall")
    fun startCall(@Body body: StartCallRequestDto): Outcome<CallResponseDto>

    @POST("home.v1.IntercomService/Answer")
    fun answer(@Body body: CallRequestDto): Outcome<CallResponseDto>

    @POST("home.v1.IntercomService/Hangup")
    fun hangup(@Body body: HangupRequestDto): Outcome<CallResponseDto>

    @POST("home.v1.IntercomService/SendSignal")
    fun sendSignal(@Body body: SendSignalRequestDto): Outcome<DeliveredResponseDto>

    @POST("home.v1.IntercomService/GetIceConfig")
    fun getIceConfig(@Body body: EmptyBody): Outcome<IceConfigResponseDto>

    @POST("home.v1.IntercomService/GetStatus")
    fun getStatus(@Body body: EmptyBody): Outcome<IntercomStatusDto>
}

/** The home control service's read-only catalog. */
internal interface ControlApi {
    @GET("v1/catalog")
    fun catalog(): Outcome<ControlCatalogDto>
}

/**
 * Wraps the kotlinx converter so no response can exceed [MAX_RESPONSE_BYTES],
 * even when the server omits Content-Length.
 */
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
                checkResponseSize(it)
                converter.convert(it)
            }
        }
    }
}

internal fun checkResponseSize(body: ResponseBody) = when {
    body.contentLength() > MAX_RESPONSE_BYTES -> throw ResponseTooLargeException()
    body.source().request(MAX_RESPONSE_BYTES + 1L) -> throw ResponseTooLargeException()
    else -> Unit
}

/** Keeps `Content-Type: application/json` exactly; String bodies would add a charset. */
private class PlainJsonBody(
    private val delegate: RequestBody,
    private val contentType: MediaType,
) : RequestBody() {
    override fun contentType(): MediaType = contentType

    override fun contentLength(): Long = delegate.contentLength()

    override fun writeTo(sink: BufferedSink) = delegate.writeTo(sink)
}

/**
 * Builds the Retrofit instances. Each gets its own OkHttpClient derived from
 * [baseClient] (shared connection pool and dispatcher) with:
 *  1. [ConnectionInterceptor] (endpoint, auth, timeouts from settings),
 *  2. [TokenRefreshAuthenticator] (one retry when the stored token rotated),
 *  3. the caller's [extraInterceptors] (logging, inspection, ...), which
 *     therefore see the final request.
 * Redirects are never followed: a bearer token must not follow a Location.
 * Responses are decoded by kotlinx serialization and adapted to [Outcome] so an
 * endpoint never sees HTTP status or JSON trees.
 */
internal object HomeRetrofit {
    // Placeholder only; ConnectionInterceptor rewrites every URL.
    private const val PLACEHOLDER_BASE_URL = "http://localhost/"
    private val JSON_MEDIA_TYPE = "application/json".toMediaType()

    fun <T : Any> create(
        api: Class<T>,
        settings: ConnectionSettingsStore,
        baseClient: OkHttpClient,
        target: Target,
        extraInterceptors: List<Interceptor>,
    ): T {
        val client = baseClient.newBuilder()
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            .addInterceptor(ConnectionInterceptor(settings, target))
            .authenticator(TokenRefreshAuthenticator(settings))
            .apply { extraInterceptors.forEach(::addInterceptor) }
            .build()
        return Retrofit.Builder()
            .baseUrl(PLACEHOLDER_BASE_URL)
            .client(client)
            .addConverterFactory(
                BoundedResponseConverterFactory(WireJson.asConverterFactory(JSON_MEDIA_TYPE), JSON_MEDIA_TYPE),
            )
            .addCallAdapterFactory(OutcomeCallAdapterFactory(WireJson))
            .build()
            .create(api)
    }
}

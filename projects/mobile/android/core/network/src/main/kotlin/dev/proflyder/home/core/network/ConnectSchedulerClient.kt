package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.Timer
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

/** Transport-level contract used by feature repositories. */
interface SchedulerClient {
    suspend fun getStatus(): Outcome<SchedulerStatus>

    suspend fun listSchedules(includeDisabled: Boolean, kind: ScheduleKind?): Outcome<List<Schedule>>
    suspend fun getSchedule(id: String): Outcome<Schedule>
    suspend fun createSchedule(draft: ScheduleDraft): Outcome<Schedule>
    suspend fun updateSchedule(schedule: Schedule, expectedRevision: Long): Outcome<Schedule>
    suspend fun deleteSchedule(id: String): Outcome<Boolean>
    suspend fun setScheduleEnabled(id: String, enabled: Boolean): Outcome<Schedule>
    suspend fun snoozeSchedule(id: String, minutes: Int): Outcome<Schedule>
    suspend fun stopSchedule(id: String): Outcome<Schedule>

    suspend fun startTimer(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer>
    suspend fun listTimers(includeFinished: Boolean): Outcome<List<Timer>>
    suspend fun pauseTimer(id: String): Outcome<Timer>
    suspend fun resumeTimer(id: String): Outcome<Timer>
    suspend fun cancelTimer(id: String): Outcome<Timer>

    /**
     * Durably queues a generic task for an executor group. Returns the event id
     * once the scheduler accepted it; this is "queued", not "executed".
     */
    suspend fun enqueueTask(
        taskType: String,
        queue: String,
        payloadJson: String,
        idempotencyKey: String,
    ): Outcome<String>
}

/**
 * Connect unary JSON/HTTP client for ricardo.v1.SchedulerService.
 *
 * Security properties:
 *  - redirects (HTTP and HTTPS) are disabled; a bearer token can never be
 *    forwarded to a Location host,
 *  - the token travels only in the Authorization header, never in the URL,
 *  - the system TLS trust store is used; there is no permissive trust manager,
 *  - no socket is bound to Wi-Fi, so Tailscale/system routing works unchanged.
 */
class ConnectSchedulerClient(
    private val settings: ConnectionSettingsStore,
    private val baseClient: OkHttpClient,
    private val serverClock: ServerClock,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : SchedulerClient {

    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = false
    }

    private val clients = ConcurrentHashMap<TimeoutKey, OkHttpClient>()

    private data class TimeoutKey(val connectMillis: Long, val readMillis: Long)

    private fun clientFor(config: ConnectionConfig): OkHttpClient {
        // Defensive clamping: corrupted persisted settings must never make
        // OkHttp's builder throw while constructing a timeout.
        val connectMillis = config.connectTimeoutMillis.coerceIn(
            ConnectionConfig.MIN_TIMEOUT_MILLIS,
            ConnectionConfig.MAX_TIMEOUT_MILLIS,
        )
        val readMillis = config.readTimeoutMillis.coerceIn(
            ConnectionConfig.MIN_TIMEOUT_MILLIS,
            ConnectionConfig.MAX_TIMEOUT_MILLIS,
        )
        val key = TimeoutKey(connectMillis, readMillis)
        return clients.computeIfAbsent(key) {
            baseClient.newBuilder()
                .connectTimeout(connectMillis, java.util.concurrent.TimeUnit.MILLISECONDS)
                .readTimeout(readMillis, java.util.concurrent.TimeUnit.MILLISECONDS)
                .writeTimeout(readMillis, java.util.concurrent.TimeUnit.MILLISECONDS)
                .callTimeout(
                    connectMillis + readMillis,
                    java.util.concurrent.TimeUnit.MILLISECONDS,
                )
                .followRedirects(false)
                .followSslRedirects(false)
                .retryOnConnectionFailure(false)
                .build()
        }
    }

    override suspend fun getStatus(): Outcome<SchedulerStatus> =
        call("GetStatus", buildJsonObject { }) { element ->
            SchedulerJson.parseStatus(element).also { status ->
                serverClock.sync(status.nowUnix)
            }
        }

    override suspend fun listSchedules(includeDisabled: Boolean, kind: ScheduleKind?): Outcome<List<Schedule>> =
        call(
            "ListSchedules",
            buildJsonObject {
                put("includeDisabled", includeDisabled)
                if (kind != null && kind != ScheduleKind.UNSPECIFIED) put("kind", "SCHEDULE_KIND_${kind.name}")
            },
        ) { element -> decodeSchedules(element) }

    override suspend fun createSchedule(draft: ScheduleDraft): Outcome<Schedule> =
        call("CreateSchedule", SchedulerJson.createScheduleRequest(draft)) { element ->
            SchedulerJson.parseSchedule(element.jsonObject.getValue("schedule"))
        }

    override suspend fun getSchedule(id: String): Outcome<Schedule> =
        call("GetSchedule", buildJsonObject { put("id", id) }) { element ->
            SchedulerJson.parseSchedule(element.jsonObject.getValue("schedule"))
        }

    override suspend fun updateSchedule(schedule: Schedule, expectedRevision: Long): Outcome<Schedule> =
        call("UpdateSchedule", SchedulerJson.updateScheduleRequest(schedule, expectedRevision)) { element ->
            SchedulerJson.parseSchedule(element.jsonObject.getValue("schedule"))
        }

    override suspend fun deleteSchedule(id: String): Outcome<Boolean> =
        call("DeleteSchedule", buildJsonObject { put("id", id) }) { element ->
            element.jsonObject["deleted"]?.jsonPrimitive?.content == "true"
        }

    override suspend fun setScheduleEnabled(id: String, enabled: Boolean): Outcome<Schedule> =
        call("SetScheduleEnabled", buildJsonObject { put("id", id); put("enabled", enabled) }) { element ->
            SchedulerJson.parseSchedule(element.jsonObject.getValue("schedule"))
        }

    override suspend fun snoozeSchedule(id: String, minutes: Int): Outcome<Schedule> =
        call("SnoozeSchedule", buildJsonObject { put("id", id); put("minutes", minutes) }) { element ->
            SchedulerJson.parseSchedule(element.jsonObject.getValue("schedule"))
        }

    override suspend fun stopSchedule(id: String): Outcome<Schedule> =
        call("StopSchedule", buildJsonObject { put("id", id) }) { element ->
            SchedulerJson.parseSchedule(element.jsonObject.getValue("schedule"))
        }

    override suspend fun startTimer(
        label: String,
        durationSeconds: Long,
        idempotencyKey: String,
    ): Outcome<Timer> = call(
        "StartTimer",
        buildJsonObject {
            put("label", label)
            // int64 is a decimal string in protobuf JSON.
            put("durationSeconds", durationSeconds.toString())
            if (idempotencyKey.isNotBlank()) put("idempotencyKey", idempotencyKey)
        },
    ) { element ->
        SchedulerJson.parseTimer(element.jsonObject.getValue("timer"))
    }

    override suspend fun listTimers(includeFinished: Boolean): Outcome<List<Timer>> =
        call("ListTimers", buildJsonObject { put("includeFinished", includeFinished) }) { element ->
            decodeTimers(element)
        }

    override suspend fun pauseTimer(id: String): Outcome<Timer> = timerAction("PauseTimer", id)
    override suspend fun resumeTimer(id: String): Outcome<Timer> = timerAction("ResumeTimer", id)
    override suspend fun cancelTimer(id: String): Outcome<Timer> = timerAction("CancelTimer", id)

    override suspend fun enqueueTask(
        taskType: String,
        queue: String,
        payloadJson: String,
        idempotencyKey: String,
    ): Outcome<String> = call(
        "EnqueueTask",
        buildJsonObject {
            put("taskType", taskType)
            put("payloadJson", payloadJson)
            put("queue", queue)
            if (idempotencyKey.isNotBlank()) put("idempotencyKey", idempotencyKey)
        },
    ) { element ->
        element.jsonObject.getValue("event").jsonObject.getValue("id").jsonPrimitive.content
    }

    private suspend fun timerAction(method: String, id: String): Outcome<Timer> =
        call(method, buildJsonObject { put("id", id) }) { element ->
            SchedulerJson.parseTimer(element.jsonObject.getValue("timer"))
        }

    private suspend fun <T> call(
        method: String,
        body: JsonObject,
        decode: (JsonElement) -> T,
    ): Outcome<T> = withContext(ioDispatcher) {
        val config = settings.config.value
        val endpoint = config.endpoint ?: return@withContext Outcome.Failure(RemoteFailure.NotConfigured)
        val token = config.bearerToken?.trim()
        if (!ConnectionConfig.isTokenValid(token)) {
            return@withContext Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
        }

        val url = endpoint.rpcUrl(SchedulerJson.SERVICE, method).toHttpUrlOrNull()
            ?: return@withContext Outcome.Failure(
                RemoteFailure.Validation(dev.proflyder.home.core.model.Field.ENDPOINT),
            )

        val request = Request.Builder()
            .url(url)
            .post(body.toString().toByteArray(Charsets.UTF_8).toRequestBody(JSON_MEDIA_TYPE))
            .header("Accept", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("User-Agent", USER_AGENT)
            .header("Authorization", "Bearer $token")
            .build()

        val call = clientFor(config).newCall(request)
        val response = try {
            call.await()
        } catch (e: IOException) {
            return@withContext Outcome.Failure(e.toRemoteFailure())
        }

        response.use { result ->
            val code = result.code
            if (code in 300..399) {
                // Deliberately do not read Location: following it could leak
                // the bearer token to another host.
                return@use Outcome.Failure(
                    RemoteFailure.ProtocolFailure("HTTP $code redirect refused"),
                )
            }
            val text = try {
                readBoundedBody(result)
            } catch (_: ResponseTooLargeException) {
                return@use Outcome.Failure(RemoteFailure.ProtocolFailure("response exceeds 8 MiB limit"))
            } catch (e: IOException) {
                return@use Outcome.Failure(e.toRemoteFailure())
            }

            if (code == 401 || code == 403) {
                return@use Outcome.Failure(RemoteFailure.Unauthorized)
            }
            if (code !in 200..299) {
                return@use Outcome.Failure(parseConnectError(code, text))
            }
            if (text.isBlank()) {
                return@use Outcome.Failure(RemoteFailure.ProtocolFailure("empty response body"))
            }
            val element = try {
                json.parseToJsonElement(text)
            } catch (_: Exception) {
                return@use Outcome.Failure(RemoteFailure.ProtocolFailure("invalid JSON response"))
            }
            try {
                Outcome.Success(decode(element))
            } catch (e: Exception) {
                Outcome.Failure(RemoteFailure.ProtocolFailure(sanitizeMessage(e.message) ?: "unexpected response shape"))
            }
        }
    }

    private fun parseConnectError(httpCode: Int, text: String): RemoteFailure {
        val parsed = try {
            if (text.isBlank()) null else json.parseToJsonElement(text).jsonObject
        } catch (_: Exception) {
            null
        }
        val code = sanitizeMessage((parsed?.get("code") as? JsonPrimitive)?.content).orEmpty()
        val message = sanitizeMessage((parsed?.get("message") as? JsonPrimitive)?.content).orEmpty()
        return when (code) {
            "unauthenticated", "permission_denied" -> RemoteFailure.Unauthorized
            "deadline_exceeded", "canceled" -> RemoteFailure.Timeout
            "unavailable" -> RemoteFailure.Offline(message.ifBlank { "server unavailable" })
            else -> RemoteFailure.Server(
                code = code.ifBlank { "http_$httpCode" },
                message = message.ifBlank { "server returned HTTP $httpCode" },
            )
        }
    }

    private fun decodeSchedules(element: JsonElement): List<Schedule> {
        val field = element.jsonObject["schedules"] ?: return emptyList()
        val array = field as? JsonArray ?: throw MalformedResponse("schedules must be an array")
        return array.map(SchedulerJson::parseSchedule)
    }

    private fun decodeTimers(element: JsonElement): List<Timer> {
        val field = element.jsonObject["timers"] ?: return emptyList()
        val array = field as? JsonArray ?: throw MalformedResponse("timers must be an array")
        return array.map(SchedulerJson::parseTimer)
    }

    private class MalformedResponse(message: String) : Exception(message)

    companion object {
        private val JSON_MEDIA_TYPE = "application/json".toMediaType()
        private const val USER_AGENT = "home-android/0.1.0"
    }
}

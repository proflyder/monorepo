package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import java.lang.reflect.Type
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.ResponseBody
import retrofit2.Converter
import retrofit2.Response
import retrofit2.Retrofit
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path

/** Connect unary calls of ricardo.v1.SchedulerService: POST <prefix>/ricardo.v1.SchedulerService/<Method>. */
internal interface SchedulerApi {
    @POST("ricardo.v1.SchedulerService/{method}")
    suspend fun call(@Path("method") method: String, @Body body: JsonObject): Response<JsonElement>
}

/** The home control service's read-only catalog. */
internal interface ControlApi {
    @GET("v1/catalog")
    suspend fun catalog(): Response<JsonElement>
}

/** JsonObject request bodies and JsonElement responses (bounded to 8 MiB). */
internal class JsonElementConverterFactory(private val json: Json) : Converter.Factory() {
    override fun requestBodyConverter(
        type: Type,
        parameterAnnotations: Array<out Annotation>,
        methodAnnotations: Array<out Annotation>,
        retrofit: Retrofit,
    ): Converter<*, RequestBody>? =
        if (type == JsonObject::class.java) {
            Converter<JsonObject, RequestBody> {
                it.toString().toByteArray(Charsets.UTF_8).toRequestBody(JSON_MEDIA_TYPE)
            }
        } else {
            null
        }

    override fun responseBodyConverter(
        type: Type,
        annotations: Array<out Annotation>,
        retrofit: Retrofit,
    ): Converter<ResponseBody, *>? =
        if (type == JsonElement::class.java) {
            Converter<ResponseBody, JsonElement> { body -> body.use { json.parseToJsonElement(it.readBounded()) } }
        } else {
            null
        }

    private companion object {
        val JSON_MEDIA_TYPE = "application/json".toMediaType()
    }
}

/**
 * Builds the Retrofit instances. Each gets its own OkHttpClient derived from
 * [baseClient] (shared connection pool and dispatcher) with:
 *  1. [ConnectionInterceptor] (endpoint, auth, timeouts from settings),
 *  2. the caller's [extraInterceptors] (logging, inspection, ...), which
 *     therefore see the final request.
 * Redirects are never followed: a bearer token must not follow a Location.
 */
internal object HomeRetrofit {
    // Placeholder only; ConnectionInterceptor rewrites every URL.
    private const val PLACEHOLDER_BASE_URL = "http://localhost/"

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
            .apply { extraInterceptors.forEach(::addInterceptor) }
            .build()
        return Retrofit.Builder()
            .baseUrl(PLACEHOLDER_BASE_URL)
            .client(client)
            .addConverterFactory(JsonElementConverterFactory(Json { ignoreUnknownKeys = true }))
            .build()
            .create(api)
    }
}

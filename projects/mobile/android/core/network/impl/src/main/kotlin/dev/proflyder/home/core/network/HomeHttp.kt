package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.concurrent.ConcurrentHashMap
import kotlin.reflect.KClass
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import retrofit2.converter.kotlinx.serialization.asConverterFactory

@SingleIn(AppScope::class)
@Inject
class HomeHttp(
    private val baseClient: OkHttpClient,
    private val settings: ConnectionSettingsStore,
    @NetworkInspection private val inspection: Set<Interceptor>,
    private val gate: RouteGate,
    private val version: AppVersion,
    private val scope: CoroutineScope,
    @IoDispatcher private val io: CoroutineDispatcher,
) {
    private val userAgent by lazy { "home-android/${version.name}" }
    private val clients = ConcurrentHashMap<Pair<Target, Boolean>, OkHttpClient>()
    private val callAdapters by lazy { OutcomeCallAdapterFactory(gate, scope) }
    private val converters by lazy {
        BoundedResponseConverterFactory(NetworkJson.asConverterFactory(JSON_MEDIA_TYPE), JSON_MEDIA_TYPE)
    }

    internal fun <T : Any> rest(api: KClass<T>, target: Target): T = Retrofit.Builder()
        .baseUrl(PLACEHOLDER_BASE_URL)
        .client(client(target, streaming = false))
        .addConverterFactory(converters)
        .addCallAdapterFactory(callAdapters)
        .build()
        .create(api.java)

    internal fun <T> stream(target: Target, request: StreamRequest, decoder: StreamDecoder<T>): Flow<T> = flow {
        gate.await()
        val call = client(target, streaming = true).newCall(request.toOkHttp(PLACEHOLDER_BASE_URL))
        call.readStream(decoder) { emit(it) }
    }.flowOn(io)

    private fun client(target: Target, streaming: Boolean): OkHttpClient =
        clients.getOrPut(target to streaming) { buildClient(target, streaming) }

    private fun buildClient(target: Target, streaming: Boolean): OkHttpClient = baseClient.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .retryOnConnectionFailure(!streaming)
        .addInterceptor(ConnectionInterceptor(settings, target, userAgent, streaming))
        .addInterceptor(TimeZoneInterceptor())
        .authenticator(TokenRefreshAuthenticator(settings))
        .apply { if (!streaming) inspection.forEach(::addInterceptor) }
        .build()

    private companion object {
        const val PLACEHOLDER_BASE_URL = "http://localhost/"
        val JSON_MEDIA_TYPE = "application/json".toMediaType()
    }
}

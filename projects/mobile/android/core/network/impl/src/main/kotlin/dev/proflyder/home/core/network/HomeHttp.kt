package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.DeviceIdentity
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
    private val identity: DeviceIdentity,
    private val forbidden: () -> Set<ModuleForbiddenSink>,
) {
    private val origin by lazy { DeviceOriginInterceptor(identity) }
    private val userAgent by lazy { "home-android/${version.name}" }
    private val clients = ConcurrentHashMap<Triple<Target, Boolean, Boolean>, OkHttpClient>()
    private val callAdapters by lazy { OutcomeCallAdapterFactory(gate, scope, forbidden) }
    private val converters by lazy {
        BoundedResponseConverterFactory(NetworkJson.asConverterFactory(JSON_MEDIA_TYPE), JSON_MEDIA_TYPE)
    }

    internal fun <T : Any> rest(api: KClass<T>, target: Target, inspected: Boolean = true): T = Retrofit.Builder()
        .baseUrl(PLACEHOLDER_BASE_URL)
        .client(client(target, streaming = false, inspected = inspected))
        .addConverterFactory(converters)
        .addCallAdapterFactory(callAdapters)
        .build()
        .create(api.java)

    internal fun <T> stream(target: Target, request: StreamRequest, decoder: StreamDecoder<T>): Flow<T> = flow {
        gate.await()
        val call = client(target, streaming = true, inspected = true).newCall(request.toOkHttp(PLACEHOLDER_BASE_URL))
        call.readStream(decoder) { emit(it) }
    }.flowOn(io)

    private fun client(target: Target, streaming: Boolean, inspected: Boolean): OkHttpClient =
        clients.getOrPut(Triple(target, streaming, inspected)) { buildClient(target, streaming, inspected) }

    private fun buildClient(target: Target, streaming: Boolean, inspected: Boolean): OkHttpClient = baseClient.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .retryOnConnectionFailure(!streaming)
        .addInterceptor(ConnectionInterceptor(settings, target, userAgent, streaming))
        .addInterceptor(TimeZoneInterceptor())
        .apply { if (target.connectProtocol) addInterceptor(origin) }
        .authenticator(TokenRefreshAuthenticator(settings))
        .apply { if (!streaming && inspected) inspection.forEach(::addInterceptor) }
        .build()

    private companion object {
        const val PLACEHOLDER_BASE_URL = "http://localhost/"
        val JSON_MEDIA_TYPE = "application/json".toMediaType()
    }
}

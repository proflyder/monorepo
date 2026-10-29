package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import java.util.concurrent.TimeUnit
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Interceptor
import okhttp3.Response

/** Which configured server a client talks to. */
internal enum class Target { SCHEDULER, CONTROL }

/**
 * Applies the *current* user settings to every request, so the Retrofit
 * interfaces only declare relative paths:
 *  - rewrites the URL onto the configured endpoint (including a path prefix),
 *  - adds the bearer token (header only, never in the URL) and fixed headers,
 *  - applies connect/read/write timeouts from the settings.
 *
 * Missing or unusable settings fail *before* any network I/O with a
 * [RemoteFailureException], which clients map to a controlled domain failure.
 * It is the first application interceptor, so logging and inspection
 * interceptors added after it see the final URL and headers.
 */
internal class ConnectionInterceptor(
    private val settings: ConnectionSettingsStore,
    private val target: Target,
) : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val config = settings.config.value
        val endpointField = if (target == Target.SCHEDULER) Field.ENDPOINT else Field.CONTROL_ENDPOINT
        val endpoint = when (target) {
            Target.SCHEDULER -> config.endpoint
            Target.CONTROL -> config.controlEndpoint
        } ?: throw RemoteFailureException(
            if (target == Target.SCHEDULER) RemoteFailure.NotConfigured else RemoteFailure.Validation(endpointField),
        )
        val token = config.bearerToken?.trim()
        if (!ConnectionConfig.isTokenValid(token)) {
            throw RemoteFailureException(RemoteFailure.Validation(Field.TOKEN))
        }
        val base = endpoint.baseUrl().toHttpUrlOrNull()
            ?: throw RemoteFailureException(RemoteFailure.Validation(endpointField))

        val original = chain.request()
        val url = base.newBuilder()
            .addPathSegments(original.url.encodedPath.removePrefix("/"))
            .build()
        val request = original.newBuilder()
            .url(url)
            .header("Accept", "application/json")
            .header("User-Agent", USER_AGENT)
            .header("Authorization", "Bearer $token")
            .apply { if (target == Target.SCHEDULER) header("Connect-Protocol-Version", "1") }
            .build()

        // Defensive clamping: corrupted persisted settings must never make
        // OkHttp reject a timeout.
        val connectMillis = config.connectTimeoutMillis
            .coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS)
        val readMillis = config.readTimeoutMillis
            .coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS)
        return chain
            .withConnectTimeout(connectMillis.toInt(), TimeUnit.MILLISECONDS)
            .withReadTimeout(readMillis.toInt(), TimeUnit.MILLISECONDS)
            .withWriteTimeout(readMillis.toInt(), TimeUnit.MILLISECONDS)
            .proceed(request)
    }

    private companion object {
        const val USER_AGENT = "home-android/0.1.0"
    }
}

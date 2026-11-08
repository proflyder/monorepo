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
internal enum class Target { SCHEDULER, CONTROL, INTERCOM }

/**
 * Internal request marker: a long-lived server stream (connect `Watch`) must not
 * inherit the unary read timeout. The interceptor removes the header.
 */
internal const val STREAM_REQUEST_HEADER = "X-Home-Connect-Stream"

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
        val endpointField = when (target) {
            Target.SCHEDULER -> Field.ENDPOINT
            Target.CONTROL -> Field.CONTROL_ENDPOINT
            Target.INTERCOM -> Field.INTERCOM_ENDPOINT
        }
        val endpoint = when (target) {
            Target.SCHEDULER -> config.endpoint
            Target.CONTROL -> config.controlEndpoint
            Target.INTERCOM -> config.intercomEndpoint
        } ?: throw RemoteFailureException(
            if (target == Target.SCHEDULER) {
                RemoteFailure.NotConfigured
            } else {
                RemoteFailure.Validation(endpointField)
            },
        )
        val token = config.bearerToken?.trim()
        if (!ConnectionConfig.isTokenValid(token)) {
            throw RemoteFailureException(RemoteFailure.Validation(Field.TOKEN))
        }
        val base = endpoint.baseUrl().toHttpUrlOrNull()
            ?: throw RemoteFailureException(RemoteFailure.Validation(endpointField))

        val original = chain.request()
        val streaming = original.header(STREAM_REQUEST_HEADER) != null
        val url = base.newBuilder()
            .addPathSegments(original.url.encodedPath.removePrefix("/"))
            .build()
        val request = original.newBuilder()
            .url(url)
            .header("Accept", original.header("Accept") ?: "application/json")
            .header("User-Agent", USER_AGENT)
            .header("Authorization", "Bearer $token")
            .apply {
                if (target == Target.SCHEDULER || target == Target.INTERCOM) {
                    header("Connect-Protocol-Version", "1")
                }
                if (streaming) removeHeader(STREAM_REQUEST_HEADER)
            }
            .build()

        // Defensive clamping: corrupted persisted settings must never make
        // OkHttp reject a timeout.
        val connectMillis = config.connectTimeoutMillis
            .coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS)
        val readMillis = config.readTimeoutMillis
            .coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS)
        return chain
            .withConnectTimeout(connectMillis.toInt(), TimeUnit.MILLISECONDS)
            // A Watch stream is intentionally idle between calls; 0 disables the
            // read deadline for that request only.
            .withReadTimeout(if (streaming) 0 else readMillis.toInt(), TimeUnit.MILLISECONDS)
            .withWriteTimeout(readMillis.toInt(), TimeUnit.MILLISECONDS)
            .proceed(request)
    }

    private companion object {
        const val USER_AGENT = "home-android/0.1.0"
    }
}

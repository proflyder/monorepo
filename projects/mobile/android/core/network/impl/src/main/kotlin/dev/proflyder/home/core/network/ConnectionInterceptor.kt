package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import java.util.concurrent.TimeUnit
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Interceptor
import okhttp3.Request
import okhttp3.Response

internal class ConnectionInterceptor(
    private val settings: ConnectionSettingsStore,
    private val target: Target,
    private val userAgent: String,
    private val streaming: Boolean,
) : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val config = settings.config.value
        val request = chain.request().routedTo(baseUrl(config), bearerToken(config))
        val readMillis = config.readTimeoutMillis.boundedTimeout()
        return chain
            .withConnectTimeout(config.connectTimeoutMillis.boundedTimeout(), TimeUnit.MILLISECONDS)
            .withReadTimeout(if (streaming) 0 else readMillis, TimeUnit.MILLISECONDS)
            .withWriteTimeout(readMillis, TimeUnit.MILLISECONDS)
            .proceed(request)
    }

    private fun baseUrl(config: ConnectionConfig): HttpUrl {
        val endpoint = target.endpointOf(config) ?: throw RemoteFailureException(target.missingEndpoint)
        return endpoint.baseUrl().toHttpUrlOrNull()
            ?: throw RemoteFailureException(RemoteFailure.Validation(target.endpointField))
    }

    private fun bearerToken(config: ConnectionConfig): String {
        val token = config.bearerToken?.trim()
        if (!ConnectionConfig.isTokenValid(token)) throw RemoteFailureException(RemoteFailure.Validation(Field.TOKEN))
        return requireNotNull(token)
    }

    private fun Request.routedTo(base: HttpUrl, token: String): Request {
        val routed = base.newBuilder()
            .addPathSegments(url.encodedPath.removePrefix("/"))
            .encodedQuery(url.encodedQuery)
            .build()
        return newBuilder()
            .url(routed)
            .header("Accept", header("Accept") ?: "application/json")
            .header("User-Agent", userAgent)
            .header("Authorization", "Bearer $token")
            .apply { if (target.connectProtocol) header("Connect-Protocol-Version", "1") }
            .build()
    }

    private fun Long.boundedTimeout(): Int =
        coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS).toInt()
}

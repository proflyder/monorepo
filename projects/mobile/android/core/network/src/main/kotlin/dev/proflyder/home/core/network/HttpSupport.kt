package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.RemoteFailure
import java.io.IOException
import java.io.InterruptedIOException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.net.ssl.SSLException
import javax.net.ssl.SSLHandshakeException
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import okhttp3.ResponseBody

/** HTTP plumbing shared by the scheduler and control-catalog clients. */

internal const val MAX_RESPONSE_BYTES = 8L * 1024L * 1024L

internal class ResponseTooLargeException : IOException("response exceeds 8 MiB limit")

/**
 * A request failed before reaching the network for a reason the user can fix
 * (no endpoint, bad token). Thrown by [ConnectionInterceptor] so it travels
 * through OkHttp as an [IOException] and is mapped back by [toRemoteFailure].
 */
internal class RemoteFailureException(val failure: RemoteFailure) : IOException("request not sent: $failure")

/** Transport failure -> honest, non-secret domain failure. */
internal fun IOException.toRemoteFailure(): RemoteFailure = when (this) {
    is RemoteFailureException -> failure
    is ResponseTooLargeException -> RemoteFailure.ProtocolFailure("response exceeds 8 MiB limit")
    is SocketTimeoutException -> RemoteFailure.Timeout
    is InterruptedIOException -> RemoteFailure.Timeout
    is SSLHandshakeException -> RemoteFailure.Tls("TLS handshake failed")
    is SSLException -> RemoteFailure.Tls("TLS connection failed")
    is UnknownHostException -> RemoteFailure.Offline("host not found")
    is ConnectException -> RemoteFailure.Offline("connection refused")
    is NoRouteToHostException -> RemoteFailure.Offline("no route to host")
    else -> {
        val timedOut = cause is SocketTimeoutException ||
            message?.contains("timed out", ignoreCase = true) == true
        if (timedOut) RemoteFailure.Timeout else RemoteFailure.Offline(sanitizeMessage(message) ?: "network error")
    }
}

internal fun sanitizeMessage(message: String?): String? = message
    ?.take(200)
    ?.replace(Regex("[\\p{Cntrl}]"), " ")
    ?.replace(Regex("(?i)bearer\\s+\\S+"), "Bearer <redacted>")

/**
 * Failure of a unary Connect call before a decoded response: transport errors
 * keep their [toRemoteFailure] mapping, malformed JSON becomes a protocol
 * failure, and anything else is a bug the caller should rethrow.
 */
internal fun Throwable.toCallFailure(): RemoteFailure = when (this) {
    is IOException -> toRemoteFailure()
    is SerializationException -> RemoteFailure.ProtocolFailure("invalid JSON response")
    else -> RemoteFailure.ProtocolFailure(sanitizeMessage(message) ?: "unexpected response shape")
}

/** Reads at most [MAX_RESPONSE_BYTES]; larger bodies fail with [ResponseTooLargeException]. */
internal fun ResponseBody.readBounded(): String {
    if (contentLength() > MAX_RESPONSE_BYTES) throw ResponseTooLargeException()
    val source = source()
    if (source.request(MAX_RESPONSE_BYTES + 1L)) throw ResponseTooLargeException()
    return source.readUtf8()
}

/**
 * Parses a Connect error envelope. The standard `message` stays on the domain
 * failure for older servers; new servers attach a `home.v1.LocalizedText`
 * detail whose JSON `debug` carries a stable key and typed arguments.
 */
fun parseConnectFailure(httpCode: Int, text: String, json: Json): RemoteFailure {
    val parsed = runCatching {
        if (text.isBlank()) null else json.decodeFromString(ConnectErrorDto.serializer(), text)
    }.getOrNull()
    val code = sanitizeMessage(parsed?.code).orEmpty()
    val message = sanitizeMessage(parsed?.message).orEmpty()
    val localized = parsed?.details?.firstNotNullOfOrNull { detail ->
        detail.debug?.takeIf { it.key.isNotBlank() }?.let { it.key to it.args }
    }
    return when (code) {
        "unauthenticated", "permission_denied" -> RemoteFailure.Unauthorized
        "deadline_exceeded", "canceled" -> RemoteFailure.Timeout
        "unavailable" -> RemoteFailure.Offline(message.ifBlank { "server unavailable" })
        else -> RemoteFailure.Server(
            code = code.ifBlank { "http_$httpCode" },
            message = message.ifBlank { "server returned HTTP $httpCode" },
            textKey = localized?.first.orEmpty(),
            textArgs = localized?.second.orEmpty(),
        )
    }
}

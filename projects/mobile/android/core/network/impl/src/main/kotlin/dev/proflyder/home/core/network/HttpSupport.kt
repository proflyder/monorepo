package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure
import java.io.IOException
import java.io.InterruptedIOException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.net.ssl.SSLException
import javax.net.ssl.SSLHandshakeException
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.SerializationException
import kotlinx.serialization.Serializable
import okhttp3.ResponseBody

internal const val MAX_RESPONSE_BYTES = 8L * 1024L * 1024L

internal class ResponseTooLargeException : IOException("response exceeds 8 MiB limit")

internal class RemoteFailureException(val failure: RemoteFailure) : IOException("request not sent: $failure")

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

internal fun Throwable.toCallFailure(): RemoteFailure = when {
    isCallCancellation() -> throw CancellationException(message ?: "call canceled", this)
    this is IOException -> toRemoteFailure()
    this is SerializationException -> RemoteFailure.ProtocolFailure("invalid JSON response")
    else -> RemoteFailure.ProtocolFailure(sanitizeMessage(message) ?: "unexpected response shape")
}

internal fun ResponseBody.requireWithinLimit() = when {
    contentLength() > MAX_RESPONSE_BYTES -> throw ResponseTooLargeException()
    source().request(MAX_RESPONSE_BYTES + 1L) -> throw ResponseTooLargeException()
    else -> Unit
}

internal fun ResponseBody.readBounded(): String {
    requireWithinLimit()
    return source().readUtf8()
}

@Serializable
internal data class PlainErrorDto(val error: String = "")

internal fun parseConnectFailure(httpCode: Int, text: String): RemoteFailure {
    val parsed = runCatching {
        if (text.isBlank()) null else NetworkJson.decodeFromString(ConnectErrorDto.serializer(), text)
    }.getOrNull()
    val plain = runCatching {
        if (text.isBlank()) null else NetworkJson.decodeFromString(PlainErrorDto.serializer(), text)
    }.getOrNull()?.error
    val reason = sanitizeMessage(parsed?.code?.takeIf { it.isNotBlank() } ?: plain).orEmpty()
    val message = sanitizeMessage(parsed?.message?.takeIf { it.isNotBlank() } ?: plain).orEmpty()
    val localized = parsed?.details?.firstNotNullOfOrNull { detail ->
        detail.debug?.takeIf { it.key.isNotBlank() }?.let { it.key to it.args }
    }
    return when (val code = ConnectCode.fromWire(reason)) {
        ConnectCode.UNAUTHENTICATED, ConnectCode.PERMISSION_DENIED -> RemoteFailure.Unauthorized
        ConnectCode.DEADLINE_EXCEEDED, ConnectCode.CANCELED -> RemoteFailure.Timeout
        ConnectCode.UNAVAILABLE -> RemoteFailure.Offline(message.ifBlank { "server unavailable" })
        else -> RemoteFailure.Server(
            httpStatus = httpCode,
            code = code,
            message = message.ifBlank { "server returned HTTP $httpCode" },
            reason = reason,
            textKey = localized?.first.orEmpty(),
            textArgs = localized?.second.orEmpty(),
        )
    }
}

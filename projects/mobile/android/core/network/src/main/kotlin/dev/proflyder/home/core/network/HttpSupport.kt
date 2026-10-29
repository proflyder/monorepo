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
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Response

/** HTTP plumbing shared by the scheduler and control-catalog clients. */

internal const val MAX_RESPONSE_BYTES = 8L * 1024L * 1024L

internal class ResponseTooLargeException : Exception()

/** Transport failure -> honest, non-secret domain failure. */
internal fun IOException.toRemoteFailure(): RemoteFailure = when (this) {
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

internal fun readBoundedBody(response: Response): String {
    val body = response.body ?: return ""
    val declared = body.contentLength()
    if (declared > MAX_RESPONSE_BYTES) throw ResponseTooLargeException()
    val source = body.source()
    if (source.request(MAX_RESPONSE_BYTES + 1L)) throw ResponseTooLargeException()
    return source.readUtf8()
}

internal suspend fun Call.await(): Response = suspendCancellableCoroutine { continuation ->
    enqueue(object : Callback {
        override fun onFailure(call: Call, e: IOException) {
            if (continuation.isActive) continuation.resumeWithException(e)
        }

        override fun onResponse(call: Call, response: Response) {
            if (continuation.isActive) continuation.resume(response) else response.close()
        }
    })
    continuation.invokeOnCancellation { cancel() }
}

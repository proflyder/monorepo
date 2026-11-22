package dev.proflyder.home.core.network

import java.io.IOException
import java.io.InterruptedIOException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.SocketException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.net.ssl.SSLException
import kotlin.coroutines.cancellation.CancellationException
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

internal data class CallRetry(
    val maxAttempts: Int,
    val step: Duration,
) {
    fun allows(attempt: Int, failure: Throwable): Boolean =
        attempt < maxAttempts && failure.isRetryableTransportFailure()

    fun delayAfter(attempt: Int): Duration = step * attempt

    companion object {
        val Idempotent = CallRetry(maxAttempts = 3, step = 150.milliseconds)
        val Once = CallRetry(maxAttempts = 1, step = Duration.ZERO)
    }
}

internal fun Throwable.isRetryableTransportFailure(): Boolean = when {
    isCallCancellation() -> false
    this is RemoteFailureException -> false
    this is ResponseTooLargeException -> false
    this is SocketTimeoutException -> false
    this is UnknownHostException -> true
    this is ConnectException -> true
    this is NoRouteToHostException -> true
    this is SocketException -> true
    this is SSLException -> true
    this is IOException -> message.isTransientConnectionMessage()
    else -> false
}

internal fun Throwable.isCallCancellation(): Boolean = when {
    this is CancellationException -> true
    this is InterruptedIOException && message.equals("Canceled", ignoreCase = true) -> true
    message.equals("Canceled", ignoreCase = true) -> true
    else -> false
}

private fun String?.isTransientConnectionMessage(): Boolean = when {
    this == null -> false
    contains("unexpected end of stream", ignoreCase = true) -> true
    contains("connection reset", ignoreCase = true) -> true
    contains("connection shut down", ignoreCase = true) -> true
    contains("broken pipe", ignoreCase = true) -> true
    contains("stream was reset", ignoreCase = true) -> true
    else -> false
}

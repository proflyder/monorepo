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
import okhttp3.ConnectionPool
import okhttp3.Interceptor
import okhttp3.Request
import okhttp3.Response

fun interface NetworkWaiter {
    fun awaitAvailable(timeoutMillis: Long): Boolean

    companion object {
        val Online: NetworkWaiter = NetworkWaiter { true }
    }
}

internal class ReadRetryInterceptor(
    private val waiter: NetworkWaiter,
    private val pool: ConnectionPool,
    private val maxAttempts: Int = DEFAULT_ATTEMPTS,
    private val retryDelayMillis: Long = DEFAULT_RETRY_DELAY_MILLIS,
    private val networkWaitMillis: Long = DEFAULT_NETWORK_WAIT_MILLIS,
) : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        if (!request.isIdempotentRead()) return chain.proceed(request)
        waiter.awaitAvailable(networkWaitMillis)
        var attempt = 1
        while (true) {
            val executed = runCatching { chain.proceed(request) }
            val response = executed.getOrNull()
            if (response != null) return response
            val failure = executed.exceptionOrNull() ?: throw IOException("request produced no response")
            attempt += 1
            when {
                attempt > maxAttempts || !failure.isRetryableReadFailure() -> throw failure
                else -> {
                    pool.evictAll()
                    Thread.sleep(retryDelayMillis * (attempt - 1))
                }
            }
        }
    }

    private companion object {
        const val DEFAULT_ATTEMPTS = 3
        const val DEFAULT_RETRY_DELAY_MILLIS = 150L
        const val DEFAULT_NETWORK_WAIT_MILLIS = 3_000L
    }
}

internal fun Request.isIdempotentRead(): Boolean = when (method) {
    "GET", "HEAD" -> true
    "POST" -> when (val rpc = url.pathSegments.lastOrNull().orEmpty()) {
        "" -> false
        else -> rpc.startsWith("List") || rpc.startsWith("Get")
    }
    else -> false
}

internal fun Throwable.isRetryableReadFailure(): Boolean = when {
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

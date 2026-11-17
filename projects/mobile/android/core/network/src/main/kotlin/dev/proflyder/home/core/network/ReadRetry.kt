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

/**
 * Platform network state. The JVM build (and every unit test) is always online;
 * Android supplies an implementation that actually waits for Wi-Fi or the VPN.
 *
 * [awaitAvailable] blocks on the calling OkHttp thread, never on the UI thread.
 * It returns `false` when no network appeared within the timeout; the request
 * is still attempted so a real connectivity error stays observable.
 */
fun interface NetworkWaiter {
    fun awaitAvailable(timeoutMillis: Long): Boolean

    companion object {
        /** Used by JVM tests and by platforms without a connectivity manager. */
        val Online: NetworkWaiter = NetworkWaiter { true }
    }
}

/**
 * Retries idempotent reads (`GET`, Connect `List*`/`Get*` RPCs) after a
 * transport failure. This is what hides the red "check LAN or VPN" alert after
 * the phone wakes up: the first attempt hits a dead keep-alive socket or a
 * still-sleeping tunnel, the next attempt succeeds on a fresh connection.
 *
 * Mutations are never retried here — their idempotency is the caller's contract
 * and lives in the feature repositories, not in the transport.
 */
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
                    // A pooled connection from before Doze is the usual culprit.
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

/** `GET`/`HEAD`, or a Connect unary RPC whose method only reads (`List`, `Get`). */
internal fun Request.isIdempotentRead(): Boolean = when (method) {
    "GET", "HEAD" -> true
    "POST" -> when (val rpc = url.pathSegments.lastOrNull().orEmpty()) {
        "" -> false
        else -> rpc.startsWith("List") || rpc.startsWith("Get")
    }
    else -> false
}

/**
 * True when the failure is a real transport problem another attempt can fix.
 * Timeouts are excluded: retrying would only double the user's wait, and the
 * server already had the request.
 */
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

/**
 * A cancelled call is not an error: the coroutine requested it. OkHttp reports
 * cancellation as `IOException("Canceled")`, which must never reach the user as
 * a network failure.
 */
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

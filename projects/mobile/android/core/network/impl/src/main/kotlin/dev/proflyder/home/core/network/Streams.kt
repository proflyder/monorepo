package dev.proflyder.home.core.network

import java.io.IOException
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okio.BufferedSource

internal data class StreamRequest(
    val path: String,
    val accept: String,
    val body: ByteArray? = null,
)

internal fun interface StreamDecoder<T> {
    suspend fun decode(source: BufferedSource, emit: suspend (T) -> Unit)
}

internal fun StreamRequest.toOkHttp(baseUrl: String): Request = Request.Builder()
    .url(baseUrl + path)
    .header("Accept", accept)
    .apply { if (body == null) get() else post(body.toRequestBody(accept.toMediaType())) }
    .build()

internal suspend fun <T> Call.readStream(decoder: StreamDecoder<T>, emit: suspend (T) -> Unit) {
    val cancellation = currentCoroutineContext().job.invokeOnCompletion { cancel() }
    runCatching { awaitStream().use { response -> decoder.decode(response.body.source(), emit) } }
        .also { cancellation.dispose() }
        .getOrThrow()
}

private suspend fun Call.awaitStream(): Response = suspendCancellableCoroutine { continuation ->
    continuation.invokeOnCancellation { cancel() }
    enqueue(StreamCallback { result -> result.fold(continuation::resumeStream, continuation::resumeWithException) })
}

private fun CancellableContinuation<Response>.resumeStream(response: Response) {
    val failure = response.streamFailure() ?: return resume(response) { _, value, _ -> value.close() }
    response.close()
    resumeWithException(failure)
}

private fun Response.streamFailure(): IOException? = when {
    code == 401 || code == 403 -> StreamUnauthorizedException()
    code in 300..399 -> IOException("HTTP $code redirect refused")
    !isSuccessful -> IOException("HTTP $code")
    else -> null
}

private class StreamCallback(private val onResult: (Result<Response>) -> Unit) : Callback {
    override fun onResponse(call: Call, response: Response) = onResult(Result.success(response))

    override fun onFailure(call: Call, e: IOException) = onResult(Result.failure(e))
}

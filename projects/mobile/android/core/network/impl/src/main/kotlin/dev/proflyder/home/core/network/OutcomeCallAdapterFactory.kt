package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.operation.suspendRunCatching
import java.lang.reflect.ParameterizedType
import java.lang.reflect.Type
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import okhttp3.Request
import okio.Timeout
import retrofit2.Call
import retrofit2.CallAdapter
import retrofit2.Callback
import retrofit2.Response
import retrofit2.Retrofit
import retrofit2.awaitResponse

internal class OutcomeCallAdapterFactory(
    private val gate: RouteGate,
    private val scope: CoroutineScope,
    private val forbiddenSinks: () -> Set<ModuleForbiddenSink> = { emptySet() },
) : CallAdapter.Factory() {
    override fun get(
        returnType: Type,
        annotations: Array<out Annotation>,
        retrofit: Retrofit,
    ): CallAdapter<*, *>? {
        if (getRawType(returnType) != Call::class.java) return null
        val outcomeType = getParameterUpperBound(0, returnType as ParameterizedType)
        if (getRawType(outcomeType) != Outcome::class.java) return null
        val bodyType = getParameterUpperBound(0, outcomeType as ParameterizedType)
        val retry = if (annotations.any { it is Idempotent }) CallRetry.Idempotent else CallRetry.Once
        val detailed = annotations.any { it is DetailedServerErrors }
        return OutcomeCallAdapter<Any>(bodyType, OutcomeExecutor(gate, retry, scope, forbiddenSinks, detailed))
    }
}

private class OutcomeCallAdapter<T>(
    private val bodyType: Type,
    private val executor: OutcomeExecutor,
) : CallAdapter<T, Call<Outcome<T>>> {
    override fun responseType(): Type = bodyType

    override fun adapt(call: Call<T>): Call<Outcome<T>> = OutcomeCall(call, executor)
}

internal class OutcomeExecutor(
    private val gate: RouteGate,
    private val retry: CallRetry,
    val scope: CoroutineScope,
    private val forbiddenSinks: () -> Set<ModuleForbiddenSink> = { emptySet() },
    private val detailed: Boolean = false,
) {
    suspend fun <T> execute(call: Call<T>): Outcome<T> {
        gate.await()
        return attempt(call, 1)
    }

    private suspend fun <T> attempt(call: Call<T>, number: Int): Outcome<T> {
        val result = suspendRunCatching { call.awaitResponse() }
        val failure = result.exceptionOrNull() ?: return result.getOrThrow().toOutcome(detailed).also(::report)
        if (!retry.allows(number, failure)) return Outcome.Failure(failure.toCallFailure())
        delay(retry.delayAfter(number, failure))
        gate.await()
        return attempt(call.clone(), number + 1)
    }

    private fun report(outcome: Outcome<*>) {
        val forbidden = (outcome as? Outcome.Failure)?.failure as? RemoteFailure.ModuleForbidden ?: return
        forbiddenSinks().forEach { it.moduleForbidden(forbidden.module) }
    }
}

private class OutcomeCall<T>(
    private val delegate: Call<T>,
    private val executor: OutcomeExecutor,
) : Call<Outcome<T>> {
    private val executed = AtomicBoolean(false)
    private val job = AtomicReference<Job?>(null)

    override fun enqueue(callback: Callback<Outcome<T>>) {
        check(executed.compareAndSet(false, true)) { "call already executed" }
        job.set(executor.scope.launch { deliver(callback) })
    }

    private suspend fun deliver(callback: Callback<Outcome<T>>) {
        runCatching { executor.execute(delegate) }.fold(
            onSuccess = { callback.onResponse(this, Response.success(it)) },
            onFailure = { callback.onFailure(this, it) },
        )
    }

    override fun execute(): Response<Outcome<T>> =
        throw UnsupportedOperationException("Outcome calls are suspend-only")

    override fun isExecuted(): Boolean = executed.get()

    override fun cancel() {
        job.get()?.cancel()
        delegate.cancel()
    }

    override fun isCanceled(): Boolean = delegate.isCanceled || job.get()?.isCancelled == true

    override fun clone(): Call<Outcome<T>> = OutcomeCall(delegate.clone(), executor)

    override fun request(): Request = delegate.request()

    override fun timeout(): Timeout = delegate.timeout()
}

internal fun <T> Response<T>.toOutcome(detailed: Boolean = false): Outcome<T> = when {
    isSuccessful -> body()?.let { Outcome.Success(it) }
        ?: Outcome.Failure(RemoteFailure.ProtocolFailure("empty response body"))
    code() in 300..399 -> Outcome.Failure(RemoteFailure.ProtocolFailure("HTTP ${code()} redirect refused"))
    code() == 401 -> Outcome.Failure(RemoteFailure.Unauthorized)
    code() == 403 && !detailed -> Outcome.Failure(forbiddenOrUnauthorized(errorText()))
    else -> Outcome.Failure(
        parseConnectFailure(code(), errorText(), detailed, headers()["Retry-After"]),
    )
}

private fun forbiddenOrUnauthorized(text: String): RemoteFailure {
    val failure = parseConnectFailure(403, text)
    return failure as? RemoteFailure.ModuleForbidden ?: RemoteFailure.Unauthorized
}

private fun <T> Response<T>.errorText(): String =
    runCatching { errorBody()?.use { it.readBounded() }.orEmpty() }.getOrDefault("")

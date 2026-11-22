package dev.proflyder.home.core.runtime

import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.retryWhen

data class Backoff(
    val initial: Duration = 1.seconds,
    val max: Duration = 30.seconds,
    val factor: Double = 2.0,
) {
    init {
        require(initial.isPositive()) { "initial backoff must be positive" }
        require(max >= initial) { "max backoff must not be below initial" }
        require(factor >= 1.0) { "backoff factor must be at least 1" }
    }

    fun delayFor(attempt: Long): Duration {
        val grown = generateSequence(initial) { (it * factor).coerceAtMost(max) }
            .take(attempt.coerceIn(0L, MAX_STEPS).toInt() + 1)
            .last()
        return grown.coerceAtMost(max)
    }

    private companion object {
        const val MAX_STEPS = 64L
    }
}

fun <T> Flow<T>.retryWithBackoff(
    backoff: Backoff,
    shouldRetry: (Throwable) -> Boolean = { true },
): Flow<T> = retryWhen { cause, attempt ->
    shouldRetry(cause).also { retry -> if (retry) delay(backoff.delayFor(attempt)) }
}

fun <T> Flow<T>.reconnecting(
    backoff: Backoff,
    shouldRetry: (Throwable) -> Boolean,
): Flow<T> = flow {
    var failures = 0L
    while (true) {
        val failure = ReconnectFailure()
        emitAll(this@reconnecting.catch { failure.record(it, shouldRetry) })
        failures = if (failure.happened) failures + 1 else 0L
        delay(if (failures == 0L) backoff.initial else backoff.delayFor(failures - 1))
    }
}

private class ReconnectFailure {
    var happened: Boolean = false
        private set

    fun record(cause: Throwable, shouldRetry: (Throwable) -> Boolean) {
        if (cause is CancellationException || !shouldRetry(cause)) throw cause
        happened = true
    }
}

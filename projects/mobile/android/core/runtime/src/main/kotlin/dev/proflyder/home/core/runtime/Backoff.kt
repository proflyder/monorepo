package dev.proflyder.home.core.runtime

import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
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

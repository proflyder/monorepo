package dev.proflyder.home.core.operation

import kotlinx.coroutines.CancellationException

/**
 * `runCatching` for suspending blocks that must not turn cancellation into a
 * failure value.
 *
 * A cancelled coroutine has to keep propagating `CancellationException`;
 * treating it as an ordinary failure leaves work running after its scope died.
 * All other throwables become `Result.failure`, like `runCatching`.
 */
suspend inline fun <T> suspendRunCatching(block: suspend () -> T): Result<T> =
    runCatching { block() }.onFailure { failure ->
        if (failure is CancellationException) throw failure
    }

package dev.proflyder.home.core.operation

import kotlinx.coroutines.CancellationException

suspend inline fun <T> suspendRunCatching(block: suspend () -> T): Result<T> =
    runCatching { block() }.onFailure { failure ->
        if (failure is CancellationException) throw failure
    }

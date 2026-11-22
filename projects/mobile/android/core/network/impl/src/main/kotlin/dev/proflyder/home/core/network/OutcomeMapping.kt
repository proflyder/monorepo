package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome

internal inline fun <T, R> Outcome<T>.mapResult(transform: (T) -> R): Outcome<R> = when (this) {
    is Outcome.Success -> runCatching { transform(value) }.fold(
        onSuccess = { Outcome.Success(it) },
        onFailure = { Outcome.Failure(it.toCallFailure()) },
    )
    is Outcome.Failure -> this
}

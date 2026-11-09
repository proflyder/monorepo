package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome

/**
 * Map a successfully decoded DTO into a domain model without letting a mapping
 * bug escape the client: any exception becomes a protocol failure, exactly like
 * a malformed body.
 */
internal inline fun <T, R> Outcome<T>.mapResult(transform: (T) -> R): Outcome<R> = when (this) {
    is Outcome.Success -> runCatching { transform(value) }.fold(
        onSuccess = { Outcome.Success(it) },
        onFailure = { Outcome.Failure(it.toCallFailure()) },
    )
    is Outcome.Failure -> this
}

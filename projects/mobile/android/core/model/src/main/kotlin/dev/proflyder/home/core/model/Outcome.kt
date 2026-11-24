package dev.proflyder.home.core.model

sealed interface RemoteFailure {
    data object NotConfigured : RemoteFailure
    data object Unauthorized : RemoteFailure
    data class ModuleForbidden(val module: String) : RemoteFailure
    data class Offline(val detail: String) : RemoteFailure
    data object Timeout : RemoteFailure
    data class Tls(val detail: String) : RemoteFailure
    data class Server(
        val httpStatus: Int,
        val code: ConnectCode,
        val message: String,
        val reason: String = "",
        val textKey: String = "",
        val textArgs: Map<String, String> = emptyMap(),
    ) : RemoteFailure
    data class ProtocolFailure(val detail: String) : RemoteFailure
    data class Validation(val field: Field, val detail: String = "") : RemoteFailure
    data class Storage(val reason: StorageReason) : RemoteFailure
    data object RemoteConfig : RemoteFailure
}

enum class StorageReason {
    TOKEN_ENCRYPTION,
    SETTINGS_WRITE,
    ATTACHMENT_MISSING,
}

fun RemoteFailure.isAmbiguousRetryable(): Boolean = when (this) {
    is RemoteFailure.Offline,
    RemoteFailure.Timeout,
    is RemoteFailure.Tls,
    is RemoteFailure.Server,
    is RemoteFailure.ProtocolFailure,
    -> true
    RemoteFailure.NotConfigured,
    RemoteFailure.Unauthorized,
    is RemoteFailure.ModuleForbidden,
    is RemoteFailure.Validation,
    is RemoteFailure.Storage,
    RemoteFailure.RemoteConfig,
    -> false
}

sealed interface Outcome<out T> {
    data class Success<T>(val value: T) : Outcome<T>
    data class Failure(val failure: RemoteFailure) : Outcome<Nothing>

    fun getOrNull(): T? = (this as? Success)?.value

    fun failureOrNull(): RemoteFailure? = (this as? Failure)?.failure
}

inline fun <T, R> Outcome<T>.map(transform: (T) -> R): Outcome<R> = when (this) {
    is Outcome.Success -> Outcome.Success(transform(value))
    is Outcome.Failure -> this
}

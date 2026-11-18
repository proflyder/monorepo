package dev.proflyder.home.core.model

/**
 * A transport failure that another attempt can fix without user action: the
 * server was unreachable, the wake-up after Doze reset a keep-alive socket, or
 * a request timed out. Such a failure must not become a blocking error notice
 * right after the app returns to the foreground.
 */
fun RemoteFailure.isTransientNetwork(): Boolean = when (this) {
    is RemoteFailure.Offline,
    RemoteFailure.Timeout,
    is RemoteFailure.Tls,
    -> true
    RemoteFailure.NotConfigured,
    RemoteFailure.Unauthorized,
    is RemoteFailure.Server,
    is RemoteFailure.ProtocolFailure,
    is RemoteFailure.Validation,
    is RemoteFailure.Storage,
    -> false
}

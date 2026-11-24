package dev.proflyder.home.core.model

fun RemoteFailure.isTransientNetwork(): Boolean = when (this) {
    is RemoteFailure.Offline,
    RemoteFailure.Timeout,
    is RemoteFailure.Tls,
    -> true
    RemoteFailure.NotConfigured,
    RemoteFailure.Unauthorized,
    is RemoteFailure.ModuleForbidden,
    is RemoteFailure.Server,
    is RemoteFailure.ProtocolFailure,
    is RemoteFailure.Validation,
    is RemoteFailure.Storage,
    RemoteFailure.RemoteConfig,
    -> false
}

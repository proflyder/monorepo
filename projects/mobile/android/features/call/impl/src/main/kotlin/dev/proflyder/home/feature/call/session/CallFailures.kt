package dev.proflyder.home.feature.call.session

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.call.CallFailure

private val RemoteFailure.isBusy: Boolean get() = this is RemoteFailure.Server && code == ConnectCode.FAILED_PRECONDITION

internal fun RemoteFailure.toCallFailure(stationLookup: Boolean = false): CallFailure = when (this) {
    RemoteFailure.NotConfigured, is RemoteFailure.Validation -> CallFailure.NotConfigured
    RemoteFailure.Timeout -> CallFailure.Timeout
    is RemoteFailure.Offline -> if (stationLookup) CallFailure.StationOffline else CallFailure.Remote(this)
    is RemoteFailure.Server -> if (isBusy) CallFailure.Busy else CallFailure.Remote(this)
    RemoteFailure.Unauthorized,
    is RemoteFailure.Tls,
    is RemoteFailure.ProtocolFailure,
    is RemoteFailure.Storage,
    RemoteFailure.RemoteConfig,
    -> CallFailure.Remote(this)
}

internal fun RemoteFailure.toEndReason(): IntercomEndReason = when {
    isBusy -> IntercomEndReason.BUSY
    this is RemoteFailure.Offline -> IntercomEndReason.UNREACHABLE
    this == RemoteFailure.Timeout -> IntercomEndReason.TIMEOUT
    else -> IntercomEndReason.ERROR
}

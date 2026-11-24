package dev.proflyder.home.feature.vpn.tunnel

internal enum class ConnectOrigin(val reportsFailure: Boolean) {
    USER(reportsFailure = true),
    AUTO_POLICY(reportsFailure = false),
    NETWORK_RESTART(reportsFailure = false),
}

internal interface TunnelLauncher {
    fun launch(origin: ConnectOrigin)
}

internal object ConnectGate {
    fun allows(origin: ConnectOrigin, atHome: Boolean?): Boolean = when (origin) {
        ConnectOrigin.USER -> true
        ConnectOrigin.AUTO_POLICY -> atHome == false
        ConnectOrigin.NETWORK_RESTART -> atHome != null
    }

    fun outcome(origin: ConnectOrigin, atHome: Boolean?): ExhaustedOutcome = when {
        atHome == true -> ExhaustedOutcome.NOT_NEEDED_AT_HOME
        origin.reportsFailure -> ExhaustedOutcome.OFFER_FALLBACK
        else -> ExhaustedOutcome.QUIET
    }
}

internal enum class ExhaustedOutcome {
    NOT_NEEDED_AT_HOME,
    OFFER_FALLBACK,
    QUIET,
}

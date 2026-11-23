package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.info
import dev.proflyder.home.core.model.warn
import dev.zacsweers.metro.Inject

internal enum class DecisionSource {
    NETWORK_OR_SETTINGS,
    SYSTEM_START,
    ENVIRONMENT_TIMEOUT,
}

@Inject
internal class AutoConnectJournal(private val logger: AppLogger) {
    fun decided(source: DecisionSource, decision: AutoConnectDecision, input: AutoConnectInput) {
        val line = describe(source, decision, input)
        when (decision.action) {
            AutoConnectAction.NONE -> logger.info(TAG, line)
            else -> logger.warn(TAG, line)
        }
    }

    private fun describe(source: DecisionSource, decision: AutoConnectDecision, input: AutoConnectInput): String =
        "source=$source action=${decision.action} reason=${decision.reason} " +
            "environment=${input.environment.javaClass.simpleName} foreground=${input.foreground} " +
            "autoConnect=${input.autoConnect} connectOnOpen=${input.connectOnOpen} active=${input.active} wish=${input.userWish}"

    private companion object {
        const val TAG = "VpnAutoConnect"
    }
}

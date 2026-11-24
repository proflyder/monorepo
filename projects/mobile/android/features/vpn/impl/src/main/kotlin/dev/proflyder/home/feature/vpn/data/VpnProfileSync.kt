package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

@Inject
internal class VpnProfileSync(
    private val connection: ConnectionSettingsStore,
    private val profiles: VpnProfiles,
    private val activation: AppActivation,
    private val scope: CoroutineScope,
) : AppStartable {
    override fun start() {
        scope.launch {
            activation.awaitActive()
            connection.config
                .map { it.isConfigured && it.hasToken }
                .distinctUntilChanged()
                .filter { it }
                .collect { profiles.ensure() }
        }
    }
}

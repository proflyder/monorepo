package dev.proflyder.home.feature.vpn.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.vpn.VpnAuth
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DataStoreVpnSettingsStore(
    private val dataStore: DataStore<Preferences>,
) : VpnSettingsStore {
    private val state = MutableStateFlow(VpnSettings())

    override val settings: StateFlow<VpnSettings> = state.asStateFlow()

    override suspend fun load() {
        val preferences = suspendRunCatching { dataStore.data.first() }.getOrNull() ?: return
        state.value = VpnSettings(
            route = enumOr(preferences[KEY_ROUTE], VpnRoute.ALL),
            autoConnect = preferences[KEY_AUTO] ?: false,
            auth = enumOr(preferences[KEY_AUTH], VpnAuth.CERTIFICATE),
            certificateName = preferences[KEY_CERTIFICATE].orEmpty(),
        )
    }

    override suspend fun setRoute(route: VpnRoute) {
        save { it[KEY_ROUTE] = route.name }
        state.update { it.copy(route = route) }
    }

    override suspend fun setAutoConnect(enabled: Boolean) {
        save { it[KEY_AUTO] = enabled }
        state.update { it.copy(autoConnect = enabled) }
    }

    override suspend fun setAuth(auth: VpnAuth) {
        save { it[KEY_AUTH] = auth.name }
        state.update { it.copy(auth = auth) }
    }

    override suspend fun setCertificateName(name: String) {
        save { it[KEY_CERTIFICATE] = name }
        state.update { it.copy(certificateName = name) }
    }

    private suspend fun save(block: (androidx.datastore.preferences.core.MutablePreferences) -> Unit) {
        suspendRunCatching { dataStore.edit(block) }
    }

    private inline fun <reified T : Enum<T>> enumOr(raw: String?, fallback: T): T =
        enumValues<T>().firstOrNull { it.name == raw } ?: fallback

    private companion object {
        val KEY_ROUTE = stringPreferencesKey("vpn_route")
        val KEY_AUTO = booleanPreferencesKey("vpn_auto_connect")
        val KEY_AUTH = stringPreferencesKey("vpn_auth")
        val KEY_CERTIFICATE = stringPreferencesKey("vpn_certificate_name")
    }
}

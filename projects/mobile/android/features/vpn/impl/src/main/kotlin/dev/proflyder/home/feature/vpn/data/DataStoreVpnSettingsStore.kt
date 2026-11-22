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
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import androidx.datastore.preferences.core.emptyPreferences
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@SingleIn(AppScope::class)
@Inject
internal class DataStoreVpnSettingsStore(
    private val dataStore: DataStore<Preferences>,
    scope: CoroutineScope,
) : VpnSettingsStore {
    override val settings: StateFlow<VpnSettings?> = dataStore.data
        .catch { emit(emptyPreferences()) }
        .map(::decode)
        .stateIn(scope, SharingStarted.Eagerly, null)

    private fun decode(preferences: Preferences): VpnSettings = VpnSettings(
        route = enumOr(preferences[KEY_ROUTE], VpnRoute.ALL),
        autoConnect = preferences[KEY_AUTO] ?: false,
        connectOnOpen = preferences[KEY_ON_OPEN] ?: true,
        permissionAsked = preferences[KEY_PERMISSION_ASKED] ?: false,
        auth = enumOr(preferences[KEY_AUTH], VpnAuth.CERTIFICATE),
        certificateName = preferences[KEY_CERTIFICATE].orEmpty(),
    )

    override suspend fun setRoute(route: VpnRoute) {
        save { it[KEY_ROUTE] = route.name }
    }

    override suspend fun setAutoConnect(enabled: Boolean) {
        save { it[KEY_AUTO] = enabled }
    }

    override suspend fun setConnectOnOpen(enabled: Boolean) {
        save { it[KEY_ON_OPEN] = enabled }
    }

    override suspend fun markPermissionAsked() {
        save { it[KEY_PERMISSION_ASKED] = true }
    }

    override suspend fun setAuth(auth: VpnAuth) {
        save { it[KEY_AUTH] = auth.name }
    }

    override suspend fun setCertificateName(name: String) {
        save { it[KEY_CERTIFICATE] = name }
    }

    private suspend fun save(block: (androidx.datastore.preferences.core.MutablePreferences) -> Unit) {
        suspendRunCatching { dataStore.edit(block) }
    }

    private inline fun <reified T : Enum<T>> enumOr(raw: String?, fallback: T): T =
        enumValues<T>().firstOrNull { it.name == raw } ?: fallback

    private companion object {
        val KEY_ROUTE = stringPreferencesKey("vpn_route")
        val KEY_AUTO = booleanPreferencesKey("vpn_auto_connect")
        val KEY_ON_OPEN = booleanPreferencesKey("vpn_connect_on_open")
        val KEY_PERMISSION_ASKED = booleanPreferencesKey("vpn_permission_asked")
        val KEY_AUTH = stringPreferencesKey("vpn_auth")
        val KEY_CERTIFICATE = stringPreferencesKey("vpn_certificate_name")
    }
}

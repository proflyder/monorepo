package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.model.DeviceIdentity
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.network.VpnProfileApi
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.vpn.VpnProfileProblem
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.knownProfile
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

@SingleIn(AppScope::class)
@Inject
internal class RemoteVpnProfiles(
    private val dataStore: DataStore<Preferences>,
    private val api: VpnProfileApi,
    private val keys: VpnKeyVault,
    private val identity: DeviceIdentity,
    private val connection: ConnectionSettingsStore,
    scope: CoroutineScope,
) : VpnProfiles {
    private val mutableState = MutableStateFlow<VpnProfileState>(VpnProfileState.Loading)
    private val lock = Mutex()
    private val restored = scope.async { restore() }

    override val state: StateFlow<VpnProfileState> = mutableState.asStateFlow()

    private suspend fun restore() {
        val stored = StoredVpnProfile.decode(read())
        mutableState.value = stored?.let(VpnProfileState::Ready) ?: VpnProfileState.Missing
    }

    override suspend fun ensure(): VpnProfile? = locked {
        current() ?: request(rotate = false) ?: mutableState.value.knownProfile
    }

    override suspend fun reissue(): Boolean = locked { request(rotate = true) != null }

    private suspend fun <T> locked(block: suspend () -> T): T {
        restored.await()
        return lock.withLock { block() }
    }

    override suspend fun forget(): Boolean = locked {
        when (val removed = api.remove(identity.deviceId())) {
            is Outcome.Success -> clearLocal()
            is Outcome.Failure -> fail(VpnProblems.of(removed.failure))
        }
        mutableState.value == VpnProfileState.Missing
    }

    private fun current(): VpnProfile? = when (val value = mutableState.value) {
        is VpnProfileState.Ready -> value.profile
        else -> null
    }

    private suspend fun request(rotate: Boolean): VpnProfile? {
        val config = connection.config.value
        if (!config.isConfigured || !config.hasToken) return fail(VpnProfileProblem.NOT_CONFIGURED)
        val previous = mutableState.value.knownProfile
        mutableState.value = VpnProfileState.Loading
        val pair = if (rotate) keys.rotate() else keys.keyPair()
        val outcome = api.register(identity.deviceId(), pair.publicKey.toBase64(), identity.defaultName())
        return when (outcome) {
            is Outcome.Success -> saved(outcome.value)
            is Outcome.Failure -> fail(VpnProblems.of(outcome.failure), previous)
        }
    }

    private suspend fun saved(profile: VpnProfile): VpnProfile {
        suspendRunCatching { dataStore.edit { it[KEY_PROFILE] = StoredVpnProfile.encode(profile) } }
        mutableState.value = VpnProfileState.Ready(profile)
        return profile
    }

    private fun fail(problem: VpnProfileProblem, profile: VpnProfile? = mutableState.value.knownProfile): VpnProfile? {
        mutableState.value = VpnProfileState.Failed(problem, profile)
        return null
    }

    private suspend fun clearLocal() {
        suspendRunCatching { dataStore.edit { it.remove(KEY_PROFILE) } }
        keys.clear()
        mutableState.value = VpnProfileState.Missing
    }

    private suspend fun read(): String? = suspendRunCatching { dataStore.data.first()[KEY_PROFILE] }.getOrNull()

    private companion object {
        val KEY_PROFILE = stringPreferencesKey("vpn_profile")
    }
}

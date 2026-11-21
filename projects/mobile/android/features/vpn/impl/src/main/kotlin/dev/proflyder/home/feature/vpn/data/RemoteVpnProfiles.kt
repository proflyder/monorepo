package dev.proflyder.home.feature.vpn.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.network.VpnProfileApi
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.push.PushDeviceIdentity
import dev.proflyder.home.feature.vpn.VpnProblems
import dev.proflyder.home.feature.vpn.VpnProfileProblem
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class RemoteVpnProfiles(
    private val dataStore: DataStore<Preferences>,
    private val api: VpnProfileApi,
    private val keys: VpnKeyVault,
    private val identity: PushDeviceIdentity,
    private val connection: ConnectionSettingsStore,
) : VpnProfiles {
    private val mutableState = MutableStateFlow<VpnProfileState>(VpnProfileState.Missing)
    private val lock = Mutex()

    override val state: StateFlow<VpnProfileState> = mutableState.asStateFlow()

    override suspend fun load() {
        val stored = StoredVpnProfile.decode(read())
        mutableState.value = stored?.let(VpnProfileState::Ready) ?: VpnProfileState.Missing
    }

    override suspend fun ensure(): VpnProfile? = lock.withLock {
        current() ?: request(rotate = false) ?: known()
    }

    override suspend fun reissue(): Boolean = lock.withLock { request(rotate = true) != null }

    override suspend fun forget(): Boolean = lock.withLock {
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
        val previous = known()
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

    private fun fail(problem: VpnProfileProblem, profile: VpnProfile? = known()): VpnProfile? {
        mutableState.value = VpnProfileState.Failed(problem, profile)
        return null
    }

    private fun known(): VpnProfile? = when (val value = mutableState.value) {
        is VpnProfileState.Ready -> value.profile
        is VpnProfileState.Failed -> value.profile
        else -> null
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

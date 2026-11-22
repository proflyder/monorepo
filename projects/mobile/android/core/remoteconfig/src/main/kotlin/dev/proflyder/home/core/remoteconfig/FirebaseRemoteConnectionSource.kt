package dev.proflyder.home.core.remoteconfig

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.remoteconfig.ConfigUpdate
import com.google.firebase.remoteconfig.ConfigUpdateListener
import com.google.firebase.remoteconfig.FirebaseRemoteConfig
import com.google.firebase.remoteconfig.FirebaseRemoteConfigException
import com.google.firebase.remoteconfig.FirebaseRemoteConfigSettings
import dev.proflyder.home.core.config.REMOTE_CONNECTION_KEY
import dev.proflyder.home.core.config.RemoteConnectionSource
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.parseRemoteConnection
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.plus
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.tasks.await

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<RemoteConnectionSource>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class FirebaseRemoteConnectionSource(
    context: Context,
    appScope: CoroutineScope,
    @IoDispatcher io: CoroutineDispatcher,
) : RemoteConnectionSource, AppStartable {
    private val appContext = context.applicationContext
    private val scope = appScope + io
    private val fetchMutex = Mutex()
    private val available: Boolean = FirebaseApp.getApps(appContext).isNotEmpty()
    private val remoteConfig: FirebaseRemoteConfig by lazy { FirebaseRemoteConfig.getInstance() }

    private val mutableState = MutableStateFlow(
        if (available) RemoteConnectionState.Loading else RemoteConnectionState.Unavailable,
    )
    override val state: StateFlow<RemoteConnectionState> = mutableState.asStateFlow()

    override fun start() {
        if (!available) return
        scope.launch {
            publishCached()
            remoteConfig.addOnConfigUpdateListener(UpdateListener())
            fetch(force = false)
        }
    }

    override suspend fun refresh() {
        if (available) fetch(force = true)
    }

    private suspend fun fetch(force: Boolean) = fetchMutex.withLock {
        suspendRunCatching { fetchAndActivate(force) }.onFailure { markFailed() }
    }

    private suspend fun fetchAndActivate(force: Boolean) {
        remoteConfig.setConfigSettingsAsync(SETTINGS).await()
        remoteConfig.setDefaultsAsync(DEFAULTS).await()
        if (force) remoteConfig.fetch(0).await() else remoteConfig.fetch().await()
        remoteConfig.activate().await()
        publish()
    }

    private suspend fun activateUpdate() = fetchMutex.withLock {
        suspendRunCatching { remoteConfig.activate().await() }
            .onSuccess { publish() }
            .onFailure { markFailed() }
    }

    private fun publishCached() {
        val cached = parseRemoteConnection(remoteConfig.getString(REMOTE_CONNECTION_KEY))
        if (cached is RemoteConnectionState.Loaded) mutableState.value = cached
    }

    private fun publish() {
        mutableState.value = parseRemoteConnection(remoteConfig.getString(REMOTE_CONNECTION_KEY))
    }

    private fun markFailed() {
        mutableState.update { RemoteConnectionState.FetchFailed(it.lastKnown) }
    }

    private inner class UpdateListener : ConfigUpdateListener {
        override fun onUpdate(update: ConfigUpdate) {
            scope.launch { activateUpdate() }
        }

        override fun onError(error: FirebaseRemoteConfigException) = markFailed()
    }

    private companion object {
        val SETTINGS: FirebaseRemoteConfigSettings = FirebaseRemoteConfigSettings.Builder()
            .setMinimumFetchIntervalInSeconds(3_600)
            .build()

        val DEFAULTS: Map<String, Any> = mapOf(REMOTE_CONNECTION_KEY to "")
    }
}

private val RemoteConnectionState.lastKnown: RemoteConnectionState.Loaded?
    get() = when (this) {
        is RemoteConnectionState.Loaded -> this
        is RemoteConnectionState.FetchFailed -> lastKnown
        else -> null
    }

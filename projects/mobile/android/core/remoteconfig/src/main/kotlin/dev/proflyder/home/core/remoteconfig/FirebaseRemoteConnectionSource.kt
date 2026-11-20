package dev.proflyder.home.core.remoteconfig

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.remoteconfig.FirebaseRemoteConfigException
import com.google.firebase.remoteconfig.ConfigUpdate
import com.google.firebase.remoteconfig.ConfigUpdateListener
import com.google.firebase.remoteconfig.FirebaseRemoteConfig
import com.google.firebase.remoteconfig.FirebaseRemoteConfigSettings
import dev.proflyder.home.core.config.REMOTE_CONNECTION_KEY
import dev.proflyder.home.core.config.RemoteConnectionSource
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.parseRemoteConnection
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.tasks.await

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class FirebaseRemoteConnectionSource(context: Context) : RemoteConnectionSource {
    private val appContext = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val refreshMutex = Mutex()

    private val app: FirebaseApp? =
        if (FirebaseApp.getApps(appContext).isEmpty()) null else FirebaseApp.getApps(appContext).first()

    private val remoteConfig: FirebaseRemoteConfig? = app?.let(FirebaseRemoteConfig::getInstance)

    private val mutableState = MutableStateFlow<RemoteConnectionState>(
        if (remoteConfig == null) RemoteConnectionState.Unavailable else RemoteConnectionState.Loading,
    )
    override val state: StateFlow<RemoteConnectionState> = mutableState.asStateFlow()

    @Volatile
    private var lastLoaded: RemoteConnectionState.Loaded? = null

    init {
        val config = remoteConfig
        if (config == null) {
            mutableState.value = RemoteConnectionState.Unavailable
        } else {
            val cached = parseRemoteConnection(config.getString(REMOTE_CONNECTION_KEY))
            if (cached is RemoteConnectionState.Loaded) {
                lastLoaded = cached
                mutableState.value = cached
            }

            config.addOnConfigUpdateListener(
                object : ConfigUpdateListener {
                    override fun onUpdate(update: ConfigUpdate) {
                        scope.launch {
                            suspendRunCatching {
                                config.activate().await()
                                publish(config)
                            }.onFailure {
                                mutableState.value = RemoteConnectionState.FetchFailed(lastLoaded)
                            }
                        }
                    }

                    override fun onError(error: FirebaseRemoteConfigException) {
                        mutableState.value = RemoteConnectionState.FetchFailed(lastLoaded)
                    }
                },
            )
            scope.launch { refreshInternal(config) }
        }
    }

    override suspend fun refresh() {
        val config = remoteConfig ?: return
        refreshMutex.withLock {
            suspendRunCatching { activateAndPublish(config) }.onFailure {
                mutableState.value = RemoteConnectionState.FetchFailed(lastLoaded)
            }
        }
    }

    private suspend fun activateAndPublish(config: FirebaseRemoteConfig) {
        config.setConfigSettingsAsync(SETTINGS).await()
        config.setDefaultsAsync(DEFAULTS).await()
        config.fetch(0).await()
        config.activate().await()
        publish(config)
    }

    private suspend fun refreshInternal(config: FirebaseRemoteConfig) {
        suspendRunCatching { fetchAndPublish(config) }.onFailure {
            mutableState.value = RemoteConnectionState.FetchFailed(lastLoaded)
        }
    }

    private suspend fun fetchAndPublish(config: FirebaseRemoteConfig) {
        config.setConfigSettingsAsync(SETTINGS).await()
        config.setDefaultsAsync(DEFAULTS).await()
        config.fetchAndActivate().await()
        publish(config)
    }

    internal fun publish(config: FirebaseRemoteConfig) {
        val next = parseRemoteConnection(config.getString(REMOTE_CONNECTION_KEY))
        lastLoaded = next as? RemoteConnectionState.Loaded
        mutableState.value = next
    }

    private companion object {
        val SETTINGS: FirebaseRemoteConfigSettings = FirebaseRemoteConfigSettings.Builder()
            .setMinimumFetchIntervalInSeconds(3_600)
            .build()

        val DEFAULTS: Map<String, Any> = mapOf(REMOTE_CONNECTION_KEY to "")
    }
}

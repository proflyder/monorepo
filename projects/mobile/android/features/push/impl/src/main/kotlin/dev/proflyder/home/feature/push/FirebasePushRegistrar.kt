package dev.proflyder.home.feature.push

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.PushDevicesApi
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.tasks.await

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class FirebasePushRegistrar(
    private val context: Context,
    private val store: PushRegistrationStore,
    private val client: PushDevicesApi,
    private val publisher: PushRegistrationPublisher,
    private val identity: PushDeviceIdentity,
    private val settings: ConnectionSettingsStore,
    private val scope: CoroutineScope,
) : PushRegistrar {
    private val permissions: PushPermissions = PushPermissions(context)

    fun start() {
        PushChannels.ensure(context)
        scope.launch {
            store.load()
            registerNow()
        }
    }

    override suspend fun registerNow() {
        @Suppress("DEPRECATION")
        val token = suspendRunCatching { FirebaseMessaging.getInstance().token.await() }
            .getOrElse { return }
        registerToken(token)
    }

    suspend fun unregister(token: String) {
        if (token.isBlank()) return
        store.busy(true)
        suspendRunCatching { client.unregister(token) }
        store.busy(false)
    }

    suspend fun registerToken(token: String) {
        if (token.isBlank()) return
        if (!firebaseReady()) return
        val config = settings.config.value
        if (!config.isConfigured || !config.hasToken) return
        val name = currentDeviceName()
        store.busy(true)
        val result = suspendRunCatching {
            publisher.publish(token = token, appVersion = appVersion(), deviceName = name)
        }.getOrNull()
        store.busy(false)
        when (result) {
            is Outcome.Success -> {
                store.failed(false)
                store.savedName(name)
                store.saved(token, System.currentTimeMillis())
            }
            else -> store.failed(true)
        }
    }

    override suspend fun saveDeviceName(name: String): Boolean {
        val clean = name.trim().takeUtf8Bytes(NAME_BYTES).trim()
            .ifEmpty { identity.defaultName() }
        store.savedName(clean)
        val config = settings.config.value
        if (!config.isConfigured || !config.hasToken) return true
        if (store.registration.value.token.isNullOrBlank()) return true
        store.busy(true)
        val result = suspendRunCatching { publisher.rename(clean) }.getOrNull()
        store.busy(false)
        val renamed = result is Outcome.Success
        store.failed(!renamed)
        return renamed
    }

    override fun permissions(): PushPermissionStatus = permissions.status()

    private fun firebaseReady(): Boolean = FirebaseApp.getApps(context).isNotEmpty()

    private fun currentDeviceName(): String = store.registration.value.deviceName
        .trim().takeUtf8Bytes(NAME_BYTES).ifEmpty { identity.defaultName() }

    private fun appVersion(): String = runCatching {
        context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty()
    }.getOrDefault("").takeUtf8Bytes(NAME_BYTES)

    private companion object {
        const val NAME_BYTES = 64
    }
}

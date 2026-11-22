package dev.proflyder.home.feature.push

import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.text.takeUtf8Bytes
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
    private val identity: DeviceIdentity,
    private val settings: ConnectionSettingsStore,
    private val scope: CoroutineScope,
) : PushRegistrar {
    private val permissions: PushPermissions = PushPermissions(context)

    fun start() {
        PushChannels.ensure(context)
        scope.launch {
            store.load()
            registerCurrentToken(force = false)
        }
    }

    override suspend fun registerNow() {
        registerCurrentToken(force = true)
    }

    private suspend fun registerCurrentToken(force: Boolean) {
        @Suppress("DEPRECATION")
        val token = suspendRunCatching { FirebaseMessaging.getInstance().token.await() }
            .getOrElse { return }
        registerToken(token, force)
    }

    suspend fun unregister(token: String) {
        if (token.isBlank()) return
        store.busy(true)
        suspendRunCatching { client.unregister(token) }
        store.busy(false)
    }

    suspend fun registerToken(token: String, force: Boolean = true) {
        if (token.isBlank()) return
        if (!firebaseReady()) return
        val config = settings.config.value
        if (!config.isConfigured || !config.hasToken) return
        val wanted = PushRegistrationRequest(token, currentDeviceName(), appVersion())
        val now = System.currentTimeMillis()
        if (!force && !PushRegistrationPolicy.isDue(store.registration.value, wanted, now)) return
        publish(wanted, now)
    }

    private suspend fun publish(wanted: PushRegistrationRequest, now: Long) {
        store.busy(true)
        val result = suspendRunCatching {
            publisher.publish(
                token = wanted.token,
                appVersion = wanted.appVersion,
                deviceName = wanted.deviceName,
            )
        }.getOrNull()
        store.busy(false)
        when (result) {
            is Outcome.Success -> {
                store.failed(false)
                store.savedName(wanted.deviceName)
                store.saved(wanted.token, now, wanted.deviceName, wanted.appVersion)
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
        if (renamed) markRenamed(clean)
        return renamed
    }

    private suspend fun markRenamed(name: String) {
        val current = store.registration.value
        val token = current.token ?: return
        store.saved(token, current.registeredAtMillis, name, current.appVersion)
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

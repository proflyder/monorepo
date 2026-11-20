package dev.proflyder.home.feature.push

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class HomeMessagingService : FirebaseMessagingService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onRegistered(token: String) = register(token)

    @Deprecated("Superseded by onRegistered on FCM 26; kept for older clients.")
    override fun onNewToken(token: String) = register(token)

    private fun register(token: String) {
        val registrar = runCatching { PushEntryPoint.require().registrar }.getOrNull() ?: return
        scope.launch { registrar.registerToken(token) }
    }

    override fun onUnregistered(token: String) {
        val registrar = runCatching { PushEntryPoint.require().registrar }.getOrNull() ?: return
        scope.launch { registrar.unregister(token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val dependencies = runCatching { PushEntryPoint.require() }.getOrNull() ?: return
        val notification = message.notification
        val decoded = PushPayloads.decode(
            data = message.data,
            notificationTitle = notification?.title,
            notificationBody = notification?.body,
        )
        PushChannels.ensure(this)
        when (decoded) {
            is PushMessage.Find -> {
                dependencies.findPhone.start(decoded.reason)
                remember(dependencies, decoded)
            }
            is PushMessage.Notify -> {
                dependencies.notifications.notify(decoded)
                remember(dependencies, decoded)
            }
            null -> Unit
        }
    }

    private fun remember(dependencies: PushEntryPoint.Dependencies, message: PushMessage) {
        scope.launch { dependencies.history.record(message) }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}

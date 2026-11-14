package dev.proflyder.home.feature.push

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * FCM entry point.
 *
 * * `onRegistered` — a new or rotated token is registered immediately; a
 *   failure is retried on the next app start (`onUnregistered` forgets the old
 *   one).
 * * `onMessageReceived` — a `phone_find` **data** message starts the loud ring,
 *   anything else becomes an ordinary notification.
 *
 * Both paths work with the app killed: FCM starts this service for a
 * high-priority message, and the ring itself is a foreground service, which a
 * high-priority FCM message is allowed to start from the background. Messages
 * the system renders from a `notification` block never arrive here when the app
 * was killed; the server therefore sends find-phone as data only.
 */
class HomeMessagingService : FirebaseMessagingService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onRegistered(token: String) = register(token)

    /**
     * FCM 26 delivers new tokens through `onRegistered`; `onNewToken` is kept
     * because older Play Services releases still call it and lint requires it.
     */
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

    /** History is written off the main thread; a kept message never blocks the ring. */
    private fun remember(dependencies: PushEntryPoint.Dependencies, message: PushMessage) {
        scope.launch { dependencies.history.record(message) }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}

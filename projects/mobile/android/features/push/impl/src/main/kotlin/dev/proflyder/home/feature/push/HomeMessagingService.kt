package dev.proflyder.home.feature.push

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.proflyder.home.core.runtime.appGraph
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class HomeMessagingService : FirebaseMessagingService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @Inject
    internal lateinit var registrar: FirebasePushRegistrar

    @Inject
    internal lateinit var notifications: PushNotifier

    @Inject
    internal lateinit var findPhone: FindPhoneController

    @Inject
    internal lateinit var history: NotificationHistoryRecorder

    override fun onCreate() {
        super.onCreate()
        appGraph<PushInjector>().inject(this)
    }

    override fun onRegistered(token: String) = register(token)

    @Deprecated("Superseded by onRegistered on FCM 26; kept for older clients.")
    override fun onNewToken(token: String) = register(token)

    private fun register(token: String) {
        scope.launch { registrar.registerToken(token) }
    }

    override fun onUnregistered(token: String) {
        scope.launch { registrar.unregister(token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val notification = message.notification
        val decoded = PushPayloads.decode(
            data = message.data,
            notificationTitle = notification?.title,
            notificationBody = notification?.body,
        )
        PushChannels.ensure(this)
        when (decoded) {
            is PushMessage.Find -> {
                findPhone.start(decoded.reason)
                remember(decoded)
            }
            is PushMessage.Notify -> notify(decoded)
            null -> Unit
        }
    }

    private fun notify(message: PushMessage.Notify) {
        if (PushChatGate.shouldSkip(message)) return
        notifications.notify(message)
        remember(message)
    }

    private fun remember(message: PushMessage) {
        scope.launch { history.record(message) }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}

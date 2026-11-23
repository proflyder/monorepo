package dev.proflyder.home.feature.push

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.proflyder.home.core.data.Lease
import dev.proflyder.home.core.runtime.appGraph
import dev.proflyder.home.feature.push.di.PushHistoryFeatureScope
import dev.zacsweers.metro.Inject

class HomeMessagingService : FirebaseMessagingService() {
    @Inject
    internal lateinit var inbox: PushInbox

    @Inject
    internal lateinit var history: PushHistoryFeatureScope

    private var historyLease: Lease<*>? = null

    override fun onCreate() {
        super.onCreate()
        appGraph<PushInjector>().inject(this)
        historyLease = history.acquire()
    }

    override fun onDestroy() {
        historyLease?.close()
        historyLease = null
        super.onDestroy()
    }

    override fun onRegistered(token: String) = inbox.tokenRefreshed(token)

    @Deprecated("Superseded by onRegistered on FCM 26; kept for older clients.")
    override fun onNewToken(token: String) = inbox.tokenRefreshed(token)

    override fun onUnregistered(token: String) = inbox.tokenRevoked(token)

    override fun onMessageReceived(message: RemoteMessage) {
        val notification = message.notification
        PushPayloads.decode(
            data = message.data,
            notificationTitle = notification?.title,
            notificationBody = notification?.body,
        )?.let(inbox::received)
    }
}

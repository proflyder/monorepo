package dev.proflyder.home.feature.push

import android.content.Context
import dev.proflyder.home.core.data.SyncSignalSink
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class PushInbox internal constructor(
    private val context: Context,
    private val scope: CoroutineScope,
    private val tokens: PushTokenSource,
    private val registrar: HomePushRegistrar,
    private val notifications: PushNotifier,
    private val findPhone: FindPhoneController,
    private val history: NotificationHistoryRecorder,
    private val sync: SyncSignalSink,
) : AppStartable {
    override fun start() {
        PushChannels.ensure(context)
    }

    internal fun tokenRefreshed(token: String) {
        tokens.refreshed(token)
    }

    internal fun tokenRevoked(token: String) {
        tokens.revoked(token)
        registrar.unregister(token)
    }

    internal fun received(message: PushMessage) {
        PushChannels.ensure(context)
        when (message) {
            is PushMessage.Find -> ring(message)
            is PushMessage.Notify -> notify(message)
            is PushMessage.Sync -> sync.publish(message.signal)
        }
    }

    private fun ring(message: PushMessage.Find) {
        findPhone.start(message.reason)
        remember(message)
    }

    private fun notify(message: PushMessage.Notify) {
        if (PushChatGate.shouldSkip(message)) return
        notifications.notify(message)
        remember(message)
    }

    private fun remember(message: PushMessage.Shown) {
        scope.launch { history.record(message) }
    }
}

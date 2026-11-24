package dev.proflyder.home.feature.push

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.tasks.await

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<PushTokenSource>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class FirebasePushTokenSource(
    private val context: Context,
    private val activation: AppActivation,
    private val scope: CoroutineScope,
) : PushTokenSource, AppStartable {
    private val current = MutableStateFlow<String?>(null)

    override val token: StateFlow<String?> = current.asStateFlow()

    override fun start() {
        scope.launch {
            activation.awaitActive()
            fetch()
        }
    }

    override suspend fun fetch(): String? {
        if (FirebaseApp.getApps(context).isEmpty()) return null
        return requestToken()?.also(::refreshed) ?: current.value
    }

    override fun refreshed(token: String) {
        if (token.isNotBlank()) current.value = token
    }

    override fun revoked(token: String) {
        current.compareAndSet(token, null)
    }

    @Suppress("DEPRECATION")
    private suspend fun requestToken(): String? =
        suspendRunCatching { FirebaseMessaging.getInstance().token.await() }
            .getOrNull()
            ?.takeIf { it.isNotBlank() }
}

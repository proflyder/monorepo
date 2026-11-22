package dev.proflyder.home.feature.push

import android.content.Context
import androidx.core.content.ContextCompat
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@Inject
internal class FindPhoneController(
    private val context: Context,
    private val scope: CoroutineScope,
) {
    private val state = MutableStateFlow(FindPhoneState())

    val ringing: StateFlow<FindPhoneState> = state.asStateFlow()

    private var timeout: Job? = null

    fun start(reason: String) {
        val active = state.value
        if (active.active) {
            state.value = active.copy(reason = reason)
            FindPhoneService.refresh(context, reason)
            return
        }
        state.value = FindPhoneState(active = true, reason = reason)
        ContextCompat.startForegroundService(context, FindPhoneService.startIntent(context, reason))
        timeout?.cancel()
        timeout = scope.launch {
            delay(FindPhoneTiming.AUTO_STOP_MILLIS)
            stop()
        }
    }

    fun stop() {
        timeout?.cancel()
        timeout = null
        if (!state.value.active) {
            runCatching { context.startService(FindPhoneService.stopIntent(context)) }
            return
        }
        state.value = FindPhoneState()
        runCatching { context.startService(FindPhoneService.stopIntent(context)) }
    }

    fun isExpired(startedAtMillis: Long, nowMillis: Long): Boolean =
        FindPhoneTiming.isExpired(startedAtMillis, nowMillis)
}

internal data class FindPhoneState(
    val active: Boolean = false,
    val reason: String = "",
)

internal object FindPhoneTiming {
    const val AUTO_STOP_MILLIS: Long = 60_000

    fun remainingMillis(startedAtMillis: Long, nowMillis: Long): Long =
        (AUTO_STOP_MILLIS - (nowMillis - startedAtMillis)).coerceAtLeast(0)

    fun isExpired(startedAtMillis: Long, nowMillis: Long): Boolean =
        remainingMillis(startedAtMillis, nowMillis) == 0L
}

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

/**
 * The single source of truth for the ringing state.
 *
 * It owns the timeout: a find-phone request stops on its own after
 * [FindPhoneTiming.AUTO_STOP_MILLIS] even if nobody touches the phone, and
 * «Нашёл» stops it immediately. The foreground service only keeps the process
 * alive and shows the notification; the screen, the notification action and the
 * messaging service all call this controller.
 *
 * State is in memory: after process death the request is gone, and the next
 * push starts a fresh ring. That is deliberate — a phone ringing forever
 * because of a stale request is worse than a missed one.
 */
@SingleIn(AppScope::class)
@Inject
class FindPhoneController(
    private val context: Context,
    private val scope: CoroutineScope,
) {
    private val state = MutableStateFlow(FindPhoneState())

    /** Whether the phone is ringing now and why. */
    val ringing: StateFlow<FindPhoneState> = state.asStateFlow()

    private var timeout: Job? = null

    /** Starts ringing; a second request only updates the reason. */
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

    /** Stops ringing: sound, vibration, notification and the foreground service. */
    fun stop() {
        timeout?.cancel()
        timeout = null
        if (!state.value.active) {
            // Still make sure a service that outlived the state goes away.
            runCatching { context.startService(FindPhoneService.stopIntent(context)) }
            return
        }
        state.value = FindPhoneState()
        runCatching { context.startService(FindPhoneService.stopIntent(context)) }
    }

    /** Whether the ring should still be running, from the elapsed time. */
    fun isExpired(startedAtMillis: Long, nowMillis: Long): Boolean =
        FindPhoneTiming.isExpired(startedAtMillis, nowMillis)
}

/** Ringing state shared with the screen. */
data class FindPhoneState(
    val active: Boolean = false,
    val reason: String = "",
)

/** Pure timing rules of one ring. */
object FindPhoneTiming {
    /** Long enough to find the phone by ear, short enough not to ring forever. */
    const val AUTO_STOP_MILLIS: Long = 60_000

    /** Milliseconds left before the automatic stop; 0 once expired. */
    fun remainingMillis(startedAtMillis: Long, nowMillis: Long): Long =
        (AUTO_STOP_MILLIS - (nowMillis - startedAtMillis)).coerceAtLeast(0)

    /** Whether the ring must stop now. */
    fun isExpired(startedAtMillis: Long, nowMillis: Long): Boolean =
        remainingMillis(startedAtMillis, nowMillis) == 0L
}

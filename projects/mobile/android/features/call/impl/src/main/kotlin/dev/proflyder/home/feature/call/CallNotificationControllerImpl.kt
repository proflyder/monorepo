package dev.proflyder.home.feature.call

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** Starts the `phoneCall` foreground service when a call becomes live. */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class CallNotificationControllerImpl(
    private val context: Context,
    private val controller: CallController,
    private val scope: CoroutineScope,
) : CallNotificationController {

    private var started = false

    override fun start() {
        if (started) return
        started = true
        scope.launch {
            controller.state.collect { state ->
                if (state.isLive) {
                    ContextCompat.startForegroundService(
                        context,
                        Intent(context, CallForegroundService::class.java),
                    )
                }
            }
        }
    }
}

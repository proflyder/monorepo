package dev.proflyder.home.feature.call

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

@Inject
internal class CallForegroundStarter(
    private val context: Context,
    private val controller: CallController,
    private val scope: CoroutineScope,
) : AppStartable {
    override fun start() {
        scope.launch {
            controller.state
                .map { it.isLive }
                .distinctUntilChanged()
                .filter { it }
                .collect { startService() }
        }
    }

    private fun startService() {
        ContextCompat.startForegroundService(context, Intent(context, CallForegroundService::class.java))
    }
}

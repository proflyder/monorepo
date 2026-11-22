package dev.proflyder.home.feature.call.audio

import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.call.CallController
import dev.proflyder.home.feature.call.CallPhase
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

@Inject
internal class CallAudioSession(
    private val controller: CallController,
    private val audio: CallAudioController,
    private val scope: CoroutineScope,
) : AppStartable {
    override fun start() {
        scope.launch {
            controller.state
                .map { it.phase == CallPhase.ACTIVE }
                .distinctUntilChanged()
                .collect(::route)
        }
    }

    private fun route(active: Boolean) {
        if (active) audio.start() else audio.stop()
    }
}

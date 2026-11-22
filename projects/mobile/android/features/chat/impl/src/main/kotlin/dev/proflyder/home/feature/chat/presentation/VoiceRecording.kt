package dev.proflyder.home.feature.chat.presentation

import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.data.VoiceCapture
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal sealed interface RecordingState {
    data object Idle : RecordingState

    data class Active(val elapsedMillis: Long) : RecordingState
}

internal class VoiceRecording(
    private val capture: VoiceCapture,
    private val scope: CoroutineScope,
    private val tick: Duration = TICK,
) {
    private val mutable = MutableStateFlow<RecordingState>(RecordingState.Idle)
    private var ticker: Job? = null

    val state: StateFlow<RecordingState> = mutable.asStateFlow()

    fun start(): Boolean {
        if (state.value is RecordingState.Active || !capture.start(scope)) return false
        mutable.value = RecordingState.Active(0)
        ticker = tickerFlow(tick).onEach { mutable.value = RecordingState.Active(capture.elapsedMillis()) }.launchIn(scope)
        return true
    }

    suspend fun stop(): LocalAttachment? {
        if (state.value !is RecordingState.Active) return null
        halt()
        return capture.stop()
    }

    fun cancel() {
        halt()
        capture.cancel()
    }

    private fun halt() {
        ticker?.cancel()
        ticker = null
        mutable.value = RecordingState.Idle
    }

    companion object {
        const val MAX_MILLIS = 120_000L
        private val TICK = 250.milliseconds
    }
}

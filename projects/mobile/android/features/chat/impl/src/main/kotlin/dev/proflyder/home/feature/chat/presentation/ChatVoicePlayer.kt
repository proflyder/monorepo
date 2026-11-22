package dev.proflyder.home.feature.chat.presentation

import android.media.AudioAttributes
import android.media.MediaPlayer
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

internal interface VoicePlayer {
    val playback: StateFlow<VoicePlayback?>

    fun toggle(scope: CoroutineScope, mediaKey: String, path: String)

    fun release()
}

internal class ChatVoicePlayer : VoicePlayer {
    private val state = MutableStateFlow<VoicePlayback?>(null)

    override val playback: StateFlow<VoicePlayback?> = state.asStateFlow()

    private var player: MediaPlayer? = null
    private var key: String? = null
    private var ticker: Job? = null

    override fun toggle(scope: CoroutineScope, mediaKey: String, path: String) {
        val current = player
        when {
            current != null && key == mediaKey && current.isPlaying -> pause(current)
            current != null && key == mediaKey -> resume(scope, current)
            else -> start(scope, mediaKey, path)
        }
    }

    private fun start(scope: CoroutineScope, mediaKey: String, path: String) {
        release()
        val created = MediaPlayer()
        created.setAudioAttributes(SPEECH)
        created.setDataSource(path)
        created.setOnCompletionListener { finish() }
        created.prepare()
        player = created
        key = mediaKey
        resume(scope, created)
    }

    private fun resume(scope: CoroutineScope, current: MediaPlayer) {
        current.start()
        ticker?.cancel()
        ticker = scope.launch { tick(current) }
    }

    private fun pause(current: MediaPlayer) {
        current.pause()
        ticker?.cancel()
        publish(current)
    }

    private suspend fun tick(current: MediaPlayer) {
        while (currentCoroutineContext().isActive) {
            publish(current)
            delay(TICK_MILLIS)
        }
    }

    private fun publish(current: MediaPlayer) {
        val mediaKey = key ?: return
        state.value = (VoicePlayback(mediaKey, current.currentPosition.toLong(), current.duration.toLong(), current.isPlaying))
    }

    private fun finish() {
        release()
    }

    override fun release() {
        ticker?.cancel()
        ticker = null
        player?.release()
        player = null
        key = null
        state.value = null
    }

    private companion object {
        const val TICK_MILLIS = 200L
        val SPEECH: AudioAttributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()
    }
}

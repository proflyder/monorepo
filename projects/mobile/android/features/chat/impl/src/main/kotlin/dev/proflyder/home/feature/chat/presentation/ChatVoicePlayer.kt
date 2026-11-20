package dev.proflyder.home.feature.chat.presentation

import android.media.AudioAttributes
import android.media.MediaPlayer
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

internal class ChatVoicePlayer(
    private val onState: (VoicePlayback?) -> Unit,
) {
    private var player: MediaPlayer? = null
    private var key: String? = null
    private var ticker: Job? = null

    fun toggle(scope: CoroutineScope, mediaKey: String, file: File) {
        val current = player
        when {
            current != null && key == mediaKey && current.isPlaying -> pause(current)
            current != null && key == mediaKey -> resume(scope, current)
            else -> start(scope, mediaKey, file)
        }
    }

    private fun start(scope: CoroutineScope, mediaKey: String, file: File) {
        release()
        val created = MediaPlayer()
        created.setAudioAttributes(SPEECH)
        created.setDataSource(file.path)
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
        onState(VoicePlayback(mediaKey, current.currentPosition.toLong(), current.duration.toLong(), current.isPlaying))
    }

    private fun finish() {
        release()
    }

    fun release() {
        ticker?.cancel()
        ticker = null
        player?.release()
        player = null
        key = null
        onState(null)
    }

    private companion object {
        const val TICK_MILLIS = 200L
        val SPEECH: AudioAttributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()
    }
}

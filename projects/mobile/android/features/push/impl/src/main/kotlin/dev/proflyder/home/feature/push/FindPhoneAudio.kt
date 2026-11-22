package dev.proflyder.home.feature.push

import android.annotation.SuppressLint
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

internal class FindPhoneAudio(private val context: Context) {
    private val audioManager: AudioManager? = context.getSystemService(AudioManager::class.java)
    private var player: MediaPlayer? = null
    private var vibrator: Vibrator? = null
    private var previousVolume: Int? = null

    internal val activePlayer: MediaPlayer? get() = player

    @SuppressLint("MissingPermission")
    fun start() {
        if (player != null) return
        raiseAlarmVolume()
        player = createPlayer()
        player?.start()
        vibrate()
    }

    @SuppressLint("MissingPermission")
    fun stop() {
        player?.runCatching { stop() }
        player?.release()
        player = null
        vibrator?.cancel()
        vibrator = null
        restoreAlarmVolume()
    }

    private fun createPlayer(): MediaPlayer? {
        val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)
            ?: return null
        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ALARM)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        return runCatching {
            MediaPlayer().apply {
                setAudioAttributes(attributes)
                setDataSource(context, uri)
                isLooping = true
                prepare()
            }
        }.getOrNull()
    }

    private fun raiseAlarmVolume() {
        val manager = audioManager ?: return
        if (previousVolume == null) {
            previousVolume = manager.getStreamVolume(AudioManager.STREAM_ALARM)
        }
        val maximum = manager.getStreamMaxVolume(AudioManager.STREAM_ALARM)
        runCatching { manager.setStreamVolume(AudioManager.STREAM_ALARM, maximum, 0) }
    }

    private fun restoreAlarmVolume() {
        val manager = audioManager ?: return
        val previous = previousVolume ?: return
        previousVolume = null
        runCatching { manager.setStreamVolume(AudioManager.STREAM_ALARM, previous, 0) }
    }

    @SuppressLint("MissingPermission")
    private fun vibrate() {
        val device = vibrator ?: vibratorFor(context) ?: return
        vibrator = device
        val effect = VibrationEffect.createWaveform(Vibration.PATTERN, REPEAT_FROM_START)
        runCatching { device.vibrate(effect) }
    }

    private fun vibratorFor(context: Context): Vibrator? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(VibratorManager::class.java)?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(Vibrator::class.java)
        }

    private companion object {
        const val REPEAT_FROM_START = 0
    }
}

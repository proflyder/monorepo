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

/**
 * The loud part of find-phone.
 *
 * `USAGE_ALARM` audio plays through the alarm stream, which is exempt from the
 * silent and vibrate ringer modes, and the stream volume is raised to its
 * maximum for the duration of the ring, then restored exactly as it was.
 *
 * The ringer mode itself is never changed. Switching it to normal used to leave
 * the phone loud after the ring: restoring the alarm volume does not restore
 * silent mode. The alarm stream is enough to be heard, so silent and vibrate
 * stay exactly as the owner set them.
 *
 * Vibration repeats with the same pattern on every API level the app supports.
 * Everything is best-effort and failure-tolerant: a device without an alarm
 * ringtone or vibrator still shows the ringing screen, so the owner can press
 * «Нашёл» rather than facing a silent, broken alert.
 */
class FindPhoneAudio(private val context: Context) {
    private val audioManager: AudioManager? = context.getSystemService(AudioManager::class.java)
    private var player: MediaPlayer? = null
    private var vibrator: Vibrator? = null
    private var previousVolume: Int? = null

    /** The player of the active ring; null when nothing is playing. */
    internal val activePlayer: MediaPlayer? get() = player

    /** Starts the alarm tone and the repeating vibration. Idempotent. */
    @SuppressLint("MissingPermission") // VIBRATE is install-time and declared here.
    fun start() {
        if (player != null) return
        raiseAlarmVolume()
        player = createPlayer()
        player?.start()
        vibrate()
    }

    /** Stops sound and vibration and restores the alarm volume. */
    @SuppressLint("MissingPermission") // VIBRATE is install-time and declared here.
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

    @SuppressLint("MissingPermission") // VIBRATE is install-time and declared here.
    private fun vibrate() {
        val device = vibrator ?: vibratorFor(context) ?: return
        vibrator = device
        // minSdk is 26, so the waveform API is always available.
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

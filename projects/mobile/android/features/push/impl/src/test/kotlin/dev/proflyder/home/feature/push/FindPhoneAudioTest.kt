package dev.proflyder.home.feature.push

import android.media.AudioManager
import android.media.RingtoneManager
import android.provider.Settings
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowMediaPlayer
import org.robolectric.shadows.util.DataSource

/**
 * The ring must be audible without leaving the phone loud.
 *
 * `USAGE_ALARM` already plays through silent and vibrate modes, so the ringer
 * mode is never changed: the old code switched it to normal and restored only
 * the alarm volume, which permanently took the phone out of silent mode. The
 * alarm volume is still raised for the ring and restored exactly afterwards,
 * and a repeated start reuses the same player.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FindPhoneAudioTest {
    private val context = RuntimeEnvironment.getApplication()
    private val manager = context.getSystemService(AudioManager::class.java)

    @Before
    fun registerTheAlarmTone() {
        val duration = 1_000
        val alarm = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
            ?: Settings.System.DEFAULT_ALARM_ALERT_URI
        addTone(alarm, duration)
        addTone(Settings.System.DEFAULT_NOTIFICATION_URI, duration)
    }

    @Test
    fun silentModeIsNotTouchedAndTheAlarmVolumeIsRestored() {
        val original = 3
        manager.ringerMode = AudioManager.RINGER_MODE_SILENT
        manager.setStreamVolume(AudioManager.STREAM_ALARM, original, 0)
        val audio = FindPhoneAudio(context)

        audio.start()

        assertNotNull(audio.activePlayer)
        assertEquals(AudioManager.RINGER_MODE_SILENT, manager.ringerMode)
        assertEquals(
            manager.getStreamMaxVolume(AudioManager.STREAM_ALARM),
            manager.getStreamVolume(AudioManager.STREAM_ALARM),
        )

        audio.stop()

        assertNull(audio.activePlayer)
        assertEquals(AudioManager.RINGER_MODE_SILENT, manager.ringerMode)
        assertEquals(original, manager.getStreamVolume(AudioManager.STREAM_ALARM))
    }

    @Test
    fun vibrateModeIsNotTouched() {
        manager.ringerMode = AudioManager.RINGER_MODE_VIBRATE
        val audio = FindPhoneAudio(context)

        audio.start()
        assertEquals(AudioManager.RINGER_MODE_VIBRATE, manager.ringerMode)
        audio.stop()

        assertEquals(AudioManager.RINGER_MODE_VIBRATE, manager.ringerMode)
    }

    @Test
    fun aRepeatedStartKeepsOnePlayer() {
        val audio = FindPhoneAudio(context)

        audio.start()
        val first = audio.activePlayer
        assertNotNull(first)
        audio.start()

        assertSame(first, audio.activePlayer)
        audio.stop()
        assertNull(audio.activePlayer)
    }

    @Test
    fun stopCanBeCalledMoreThanOnce() {
        val original = 2
        manager.setStreamVolume(AudioManager.STREAM_ALARM, original, 0)
        val audio = FindPhoneAudio(context)

        audio.start()
        audio.stop()
        audio.stop()

        assertEquals(original, manager.getStreamVolume(AudioManager.STREAM_ALARM))
        assertNull(audio.activePlayer)
    }

    private fun addTone(uri: android.net.Uri, duration: Int) {
        ShadowMediaPlayer.addMediaInfo(
            DataSource.toDataSource(uri.toString()),
            ShadowMediaPlayer.MediaInfo(duration, 0),
        )
    }
}

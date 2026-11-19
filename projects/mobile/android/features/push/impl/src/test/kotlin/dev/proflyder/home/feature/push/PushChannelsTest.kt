package dev.proflyder.home.feature.push

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.core.app.NotificationManagerCompat
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * The find channel is the silent carrier of a loud request.
 *
 * Its importance and DND bypass let the full-screen request through, but it
 * must have no sound and no vibration: the ring itself is the `MediaPlayer` in
 * the foreground service, and a channel sound would play the melody a second
 * time and repeat on every notification update.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PushChannelsTest {
    @Test
    fun theFindChannelIsSilentAndBypassesDnd() {
        val context = RuntimeEnvironment.getApplication()
        PushChannels.ensure(context)

        val find = manager(context).getNotificationChannel(PushChannels.FIND)
        assertNotNull(find)
        assertEquals(NotificationManager.IMPORTANCE_HIGH, find!!.importance)
        assertTrue(find.canBypassDnd())
        assertNull(find.sound)
        assertFalse(find.shouldVibrate())
    }

    @Test
    fun theOldLoudChannelIsDeleted() {
        val context = RuntimeEnvironment.getApplication()
        val manager = manager(context)
        manager.createNotificationChannel(
            NotificationChannel(
                PushChannels.LEGACY_FIND,
                "legacy",
                NotificationManager.IMPORTANCE_HIGH,
            ),
        )

        PushChannels.ensure(context)

        assertNull(manager.getNotificationChannel(PushChannels.LEGACY_FIND))
        assertNotNull(manager.getNotificationChannel(PushChannels.FIND))
    }

    @Test
    fun theNotificationsChannelStaysOrdinary() {
        val context = RuntimeEnvironment.getApplication()
        PushChannels.ensure(context)

        val notifications = NotificationManagerCompat.from(context)
            .getNotificationChannel(PushChannels.NOTIFICATIONS)
        assertNotNull(notifications)
        assertEquals(NotificationManager.IMPORTANCE_DEFAULT, notifications!!.importance)
        assertFalse(notifications.canBypassDnd())
    }

    @Test
    // The legacy fields are deprecated in favour of channels; they are read to
    // prove that setSilent cleared the sound and vibration on the notification.
    @Suppress("DEPRECATION")
    fun theRingNotificationIsSilentAndAlertsOnce() {
        val context = RuntimeEnvironment.getApplication()
        PushChannels.ensure(context)

        val notification = PushNotifier(context, appearance(context)).ring("Рикардо")

        assertEquals(PushChannels.FIND, notification.channelId)
        assertNull(notification.sound)
        assertNull(notification.vibrate)
        assertTrue(notification.flags and Notification.FLAG_ONLY_ALERT_ONCE != 0)
        // The accent follows the chosen theme instead of a fixed colour.
        assertEquals(AppTheme.CURRENT.palette.light.primary, notification.color)
    }

    private fun appearance(context: android.content.Context) = AppearanceController(
        PreferenceDataStoreFactory.create { File(context.filesDir, "push-appearance.preferences_pb") },
    )

    private fun manager(context: android.content.Context): NotificationManager =
        context.getSystemService(NotificationManager::class.java)
}

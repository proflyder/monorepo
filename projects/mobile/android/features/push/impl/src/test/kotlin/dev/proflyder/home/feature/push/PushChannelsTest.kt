package dev.proflyder.home.feature.push

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.core.app.NotificationManagerCompat
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
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
    @Suppress("DEPRECATION")
    fun theRingNotificationIsSilentAndAlertsOnce() {
        val context = RuntimeEnvironment.getApplication()
        PushChannels.ensure(context)

        val notification = PushNotifier(context, appearance(context)).ring("Рикардо")

        assertEquals(PushChannels.FIND, notification.channelId)
        assertNull(notification.sound)
        assertNull(notification.vibrate)
        assertTrue(notification.flags and Notification.FLAG_ONLY_ALERT_ONCE != 0)
        assertNotNull(notification.fullScreenIntent)
        assertNull(notification.group)
        assertEquals(Notification.GROUP_ALERT_ALL, notification.groupAlertBehavior)
        assertEquals(AppTheme.CURRENT.palette.light.primary, notification.color)
    }

    private fun appearance(context: android.content.Context) = AppearanceController(
        PreferenceDataStoreFactory.create { File(context.filesDir, "push-appearance.preferences_pb") },
        CoroutineScope(Dispatchers.Unconfined),
        Dispatchers.Unconfined,
    )

    private fun manager(context: android.content.Context): NotificationManager =
        context.getSystemService(NotificationManager::class.java)
}

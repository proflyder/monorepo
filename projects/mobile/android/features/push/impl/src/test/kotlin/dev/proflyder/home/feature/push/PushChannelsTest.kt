package dev.proflyder.home.feature.push

import android.app.NotificationManager
import android.media.AudioAttributes
import androidx.core.app.NotificationManagerCompat
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * Both channels must be created with the properties the loud path depends on:
 * the find channel is high-importance, uses the alarm audio usage and bypasses
 * Do Not Disturb. A later change of importance would silently mute the ring, so
 * this is pinned by a test rather than by a comment.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PushChannelsTest {
    @Test
    fun theFindChannelBypassesDndAndUsesTheAlarmStream() {
        val context = RuntimeEnvironment.getApplication()
        PushChannels.ensure(context)

        val manager = NotificationManagerCompat.from(context)
        val find = manager.getNotificationChannel(PushChannels.FIND)
        assertNotNull(find)
        assertEquals(NotificationManager.IMPORTANCE_HIGH, find!!.importance)
        assertTrue(find.canBypassDnd())
        val usage = find.audioAttributes?.usage
        assertEquals(AudioAttributes.USAGE_ALARM, usage)
        assertNotNull(find.sound)
    }

    @Test
    fun theNotificationsChannelStaysOrdinary() {
        val context = RuntimeEnvironment.getApplication()
        PushChannels.ensure(context)

        val notifications = NotificationManagerCompat.from(context)
            .getNotificationChannel(PushChannels.NOTIFICATIONS)
        assertNotNull(notifications)
        assertEquals(NotificationManager.IMPORTANCE_DEFAULT, notifications!!.importance)
        assertEquals(false, notifications.canBypassDnd())
    }
}

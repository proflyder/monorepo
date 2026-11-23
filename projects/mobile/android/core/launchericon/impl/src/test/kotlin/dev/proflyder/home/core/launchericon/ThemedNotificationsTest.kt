package dev.proflyder.home.core.launchericon

import android.app.Notification
import android.graphics.Bitmap
import androidx.core.app.NotificationCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import dev.proflyder.home.core.theme.ThemedNotificationAccent
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ThemedNotificationsTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val context = RuntimeEnvironment.getApplication()
    private val appearance = appearance()
    private val themed = AliasThemedNotifications(context, appearance, ThemedNotificationAccent(context, appearance))

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun decorationFollowsTheSelectedTheme() {
        select(AppTheme.EMERALD)
        val emerald = themed.decorate(builder()).build()
        select(AppTheme.NEON)
        val neon = themed.decorate(builder()).build()

        assertEquals(AppTheme.EMERALD.palette.light.primary, emerald.color)
        assertEquals(AppTheme.NEON.palette.light.primary, neon.color)
        assertFalse(largeIcon(emerald).sameAs(largeIcon(neon)))
    }

    @Test
    fun conversationUsesTheThemeShortcutAndMessagingStyle() {
        select(AppTheme.NEON)
        val notification = themed.conversation(builder(), "Дом", "Дверь открыта").build()

        assertNotNull(notification.shortcutId)
        assertEquals(AppTheme.NEON.palette.light.primary, notification.color)
        assertEquals("Дверь открыта", messages(notification).single().text)
        assertTrue(ShortcutManagerCompat.getDynamicShortcuts(context).any { it.id == notification.shortcutId })
        assertNotEquals("", ShortcutManagerCompat.getDynamicShortcuts(context).single().shortLabel.toString())
    }

    private fun messages(notification: Notification) =
        requireNotNull(NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(notification)).messages

    private fun largeIcon(notification: Notification): Bitmap =
        requireNotNull(notification.getLargeIcon()).loadDrawable(context).let { (it as android.graphics.drawable.BitmapDrawable).bitmap }

    private fun builder() = NotificationCompat.Builder(context, "test").setSmallIcon(android.R.drawable.ic_dialog_info)

    private fun select(theme: AppTheme) = runBlocking { withTimeout(TIMEOUT) { appearance.selectTheme(theme) } }

    private fun appearance(): AppearanceController {
        val file = File(context.filesDir, "${UUID.randomUUID()}.preferences_pb")
        val store = PreferenceDataStoreFactory.create(scope = scope) { file }
        return AppearanceController(store, scope, Dispatchers.IO, Dispatchers.Unconfined)
    }

    private companion object {
        const val TIMEOUT = 5_000L
    }
}

package dev.proflyder.home.core.theme

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
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
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NotificationAccentTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val context = RuntimeEnvironment.getApplication()

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun accentFollowsSelectedTheme() = runBlocking<Unit> {
        val appearance = appearance()
        val accent = ThemedNotificationAccent(context, appearance)
        withTimeout(TIMEOUT) { appearance.selectTheme(AppTheme.EMERALD) }
        val emerald = accent.color()
        withTimeout(TIMEOUT) { appearance.selectTheme(AppTheme.NEON) }
        val neon = accent.color()

        assertEquals(AppTheme.EMERALD.palette.light.primary, emerald)
        assertEquals(AppTheme.NEON.palette.light.primary, neon)
        assertNotEquals(emerald, neon)
    }

    private fun appearance(): AppearanceController {
        val file = File(context.filesDir, "${UUID.randomUUID()}.preferences_pb")
        val store = PreferenceDataStoreFactory.create(scope = scope) { file }
        return AppearanceController(store, scope, Dispatchers.IO, Dispatchers.Unconfined)
    }

    private companion object {
        const val TIMEOUT = 5_000L
    }
}

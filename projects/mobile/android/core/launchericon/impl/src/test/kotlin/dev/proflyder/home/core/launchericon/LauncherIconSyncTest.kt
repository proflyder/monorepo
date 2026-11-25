package dev.proflyder.home.core.launchericon

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.ForegroundState
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LauncherIconSyncTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val icons = RecordingIcons()

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun themeChangeSwitchesTheIconOnlyAfterTheAppGoesToBackground() = runBlocking<Unit> {
        val appearance = appearance()
        val foreground = FakeForeground()
        LauncherIconSync(appearance, icons, foreground, scope, Dispatchers.IO).start()
        foreground.move(true)
        appearance.selectTheme(AppTheme.NEON)
        delay(SETTLE_MS)
        assertEquals("no alias switch while the task is visible", emptyList<AppTheme>(), icons.history.value)
        foreground.move(false)
        awaitShown(AppTheme.NEON)
        foreground.move(true)
        appearance.selectTheme(AppTheme.SYSTEM)
        delay(SETTLE_MS)
        assertEquals(listOf(AppTheme.NEON), icons.history.value)
        foreground.move(false)
        awaitShown(AppTheme.SYSTEM)
    }

    @Test
    fun backgroundWithTheShownThemeLeavesTheAliasesAlone() = runBlocking<Unit> {
        val foreground = FakeForeground()
        LauncherIconSync(appearance(), icons, foreground, scope, Dispatchers.IO).start()
        foreground.move(true)
        foreground.move(false)
        delay(SETTLE_MS)
        assertEquals(emptyList<AppTheme>(), icons.history.value)
    }

    private suspend fun awaitShown(theme: AppTheme) = withTimeout(TIMEOUT_MS) {
        icons.history.first { it.lastOrNull() == theme }
    }

    private fun appearance(): AppearanceController {
        val dir = File(RuntimeEnvironment.getApplication().cacheDir, UUID.randomUUID().toString())
        val store = PreferenceDataStoreFactory.create(scope = scope) { File(dir, "appearance.preferences_pb") }
        return AppearanceController(store, scope, Dispatchers.IO, Dispatchers.IO)
    }

    private class FakeForeground : AppForeground {
        override val state = MutableStateFlow(ForegroundState(foreground = false, sinceElapsedNanos = 0))

        suspend fun move(visible: Boolean) {
            delay(SETTLE_MS)
            state.value = ForegroundState(visible, state.value.sinceElapsedNanos + 1)
        }
    }

    private class RecordingIcons : LauncherIcons {
        val history = MutableStateFlow<List<AppTheme>>(emptyList())

        override fun shown(): AppTheme = history.value.lastOrNull() ?: LauncherAliases.manifestDefault

        override fun show(theme: AppTheme) {
            history.value = history.value + theme
        }
    }

    private companion object {
        const val TIMEOUT_MS = 5_000L
        const val SETTLE_MS = 200L
    }
}

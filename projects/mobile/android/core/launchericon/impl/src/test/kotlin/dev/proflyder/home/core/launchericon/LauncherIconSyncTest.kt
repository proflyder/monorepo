package dev.proflyder.home.core.launchericon

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
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
    fun startShowsStoredThemeAndFollowsChanges() = runBlocking<Unit> {
        val appearance = appearance()
        appearance.selectTheme(AppTheme.NEON)
        LauncherIconSync(appearance, icons, scope, Dispatchers.IO).start()
        awaitShown(AppTheme.NEON)
        appearance.selectTheme(AppTheme.SYSTEM)
        awaitShown(AppTheme.SYSTEM)
    }

    @Test
    fun freshStartShowsDefaultTheme() = runBlocking<Unit> {
        LauncherIconSync(appearance(), icons, scope, Dispatchers.IO).start()
        awaitShown(AppTheme.DEFAULT)
        assertEquals(listOf(AppTheme.DEFAULT), icons.history.value)
    }

    private suspend fun awaitShown(theme: AppTheme) = withTimeout(TIMEOUT_MS) {
        icons.history.first { it.lastOrNull() == theme }
    }

    private fun appearance(): AppearanceController {
        val dir = File(RuntimeEnvironment.getApplication().cacheDir, UUID.randomUUID().toString())
        val store = PreferenceDataStoreFactory.create(scope = scope) { File(dir, "appearance.preferences_pb") }
        return AppearanceController(store, scope, Dispatchers.IO, Dispatchers.IO)
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
    }
}

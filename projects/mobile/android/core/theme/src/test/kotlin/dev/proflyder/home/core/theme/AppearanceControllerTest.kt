package dev.proflyder.home.core.theme

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AppearanceControllerTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun constructionDoesNotWaitForDisk() = runBlocking<Unit> {
        val gate = CompletableDeferred<Unit>()
        val disk = dataStore()
        val slow = object : DataStore<Preferences> by disk {
            override val data: Flow<Preferences> = flow { gate.await(); emitAll(disk.data) }
        }
        val controller = controller(slow)

        assertFalse(controller.loaded.value)
        assertEquals(Appearance(), controller.state.value)
        gate.complete(Unit)
        withTimeout(TIMEOUT) { controller.loaded.first { it } }
    }

    @Test
    fun selectionIsPersistedAndRestored() = runBlocking<Unit> {
        val disk = dataStore()
        withTimeout(TIMEOUT) {
            controller(disk).selectTheme(AppTheme.NEON)
            controller(disk).selectMode(ThemeMode.DARK)
        }

        val restored = controller(disk)
        withTimeout(TIMEOUT) { restored.loaded.first { it } }
        assertEquals(Appearance(AppTheme.NEON, ThemeMode.DARK), restored.state.value)
    }

    @Test
    fun legacyCurrentKeyIsReadAsEmerald() = runBlocking<Unit> {
        val disk = dataStore()
        withTimeout(TIMEOUT) {
            disk.edit { it[stringPreferencesKey("appearance_theme")] = "current" }
        }

        val restored = controller(disk)
        withTimeout(TIMEOUT) { restored.loaded.first { it } }
        assertEquals(AppTheme.EMERALD, restored.state.value.theme)
    }

    private fun controller(dataStore: DataStore<Preferences>) =
        AppearanceController(dataStore, scope, Dispatchers.IO, Dispatchers.Unconfined)

    private fun dataStore(): DataStore<Preferences> {
        val dir = RuntimeEnvironment.getApplication().filesDir
        return PreferenceDataStoreFactory.create(scope = scope) { File(dir, "${UUID.randomUUID()}.preferences_pb") }
    }

    private companion object {
        const val TIMEOUT = 5_000L
    }
}

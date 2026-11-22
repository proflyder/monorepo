package dev.proflyder.home.core.runtime

import android.content.Context
import android.provider.Settings
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class DeviceIdentityTest {
    @Test
    fun theIdentityIsStableAndPseudonymous() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val identity = AndroidDeviceIdentity(context, store(context, "identity"))
        val first = identity.deviceId()
        val second = identity.deviceId()
        assertEquals(first, second)
        assertEquals(64, first.length)
        assertTrue(first.all { it in "0123456789abcdef" })
        val raw = Settings.Secure.getString(
            context.contentResolver,
            Settings.Secure.ANDROID_ID,
        )
        assertNotEquals(raw, first)
    }

    @Test
    fun aNewIdentityObjectReadsThePersistedValue() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "identity-restart")
        val first = AndroidDeviceIdentity(context, dataStore).deviceId()
        val second = AndroidDeviceIdentity(context, dataStore).deviceId()
        assertEquals(first, second)
    }

    @Test
    fun theDefaultNameFallsBackToANonBlankString() {
        val context = RuntimeEnvironment.getApplication()
        val identity = AndroidDeviceIdentity(context, store(context, "identity-name"))
        assertTrue(identity.defaultName().trim().isNotEmpty())
    }

    private fun store(context: Context, name: String): DataStore<Preferences> =
        PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(context.filesDir, "$name.preferences_pb") },
        )
}

package dev.proflyder.home.feature.push

import android.content.Context
import android.provider.Settings
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.PushDevicesApi
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

/** The stable device identity survives token rotation and is never the raw platform id. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PushDeviceIdentityTest {
    @Test
    fun theIdentityIsStableAndPseudonymous() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val identity = AndroidPushDeviceIdentity(context, store(context, "identity"))
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
        val first = AndroidPushDeviceIdentity(context, dataStore).deviceId()
        val second = AndroidPushDeviceIdentity(context, dataStore).deviceId()
        assertEquals(first, second)
    }

    @Test
    fun theDefaultNameFallsBackToANonBlankString() {
        val context = RuntimeEnvironment.getApplication()
        val identity = AndroidPushDeviceIdentity(context, store(context, "identity-name"))
        assertTrue(identity.defaultName().trim().isNotEmpty())
    }

    private fun store(context: Context, name: String): DataStore<Preferences> =
        PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(context.filesDir, "$name.preferences_pb") },
        )
}

/** Registration keeps one device id across FCM token rotations and name changes. */
class PushRegistrationPublisherTest {
    private class FakeApi : PushDevicesApi {
        val registrations = mutableListOf<List<String>>()
        val renames = mutableListOf<List<String>>()

        override suspend fun register(
            deviceId: String,
            token: String,
            appVersion: String,
            deviceName: String,
        ): Outcome<Unit> {
            registrations += listOf(deviceId, token, appVersion, deviceName)
            return Outcome.Success(Unit)
        }

        override suspend fun rename(deviceId: String, deviceName: String): Outcome<Unit> {
            renames += listOf(deviceId, deviceName)
            return Outcome.Success(Unit)
        }

        override suspend fun unregister(token: String): Outcome<Unit> = Outcome.Success(Unit)
    }

    private class FixedIdentity(private val id: String) : PushDeviceIdentity {
        override suspend fun deviceId(): String = id

        override fun defaultName(): String = "Pixel"
    }

    @Test
    fun aRotatedTokenIsSentWithTheSameDeviceId() = runBlocking {
        val api = FakeApi()
        val publisher = PushRegistrationPublisher(api, FixedIdentity("device-a"))
        publisher.publish(token = "token-a", appVersion = "1.0", deviceName = "Стас")
        publisher.publish(token = "token-b", appVersion = "1.0", deviceName = "Стас")
        assertEquals(2, api.registrations.size)
        assertEquals("device-a", api.registrations[0][0])
        assertEquals("device-a", api.registrations[1][0])
        assertEquals("token-a", api.registrations[0][1])
        assertEquals("token-b", api.registrations[1][1])
        assertEquals("Стас", api.registrations[1][3])
    }

    @Test
    fun renamingUsesTheSameDeviceIdWithoutAToken() = runBlocking {
        val api = FakeApi()
        val publisher = PushRegistrationPublisher(api, FixedIdentity("device-a"))
        publisher.rename("Аня")
        assertEquals(listOf(listOf("device-a", "Аня")), api.renames)
        assertTrue(api.registrations.isEmpty())
    }
}

/** The owner-visible name is persisted locally so the screen survives a restart. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PushRegistrationStoreNameTest {
    @Test
    fun theSavedNameIsReloaded() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(context.filesDir, "registration-name.preferences_pb") },
        )
        DataStorePushRegistrationStore(dataStore).savedName("Стас")
        val reloaded = DataStorePushRegistrationStore(dataStore)
        reloaded.load()
        assertEquals("Стас", reloaded.registration.value.deviceName)
    }
}

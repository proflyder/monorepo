package dev.proflyder.home.feature.push

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.PushDevicesApi
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

class PushRegistrationPublisherTest {
    private class FakeApi : PushDevicesApi {
        val registrations = mutableListOf<List<String>>()

        override suspend fun register(
            deviceId: String,
            token: String,
            appVersion: String,
        ): Outcome<Unit> {
            registrations += listOf(deviceId, token, appVersion)
            return Outcome.Success(Unit)
        }

        override suspend fun unregister(token: String): Outcome<Unit> = Outcome.Success(Unit)
    }

    private class FixedIdentity(private val id: String) : DeviceIdentity {
        override suspend fun deviceId(): String = id

        override fun defaultName(): String = "Pixel"
    }

    @Test
    fun aRotatedTokenIsSentWithTheSameDeviceId() = runBlocking {
        val api = FakeApi()
        val publisher = PushRegistrationPublisher(api, FixedIdentity("device-a"))
        publisher.publish(PushRegistrationRequest("token-a", "1.0", SERVER))
        publisher.publish(PushRegistrationRequest("token-b", "1.0", SERVER))
        assertEquals(2, api.registrations.size)
        assertEquals("device-a", api.registrations[0][0])
        assertEquals("device-a", api.registrations[1][0])
        assertEquals("token-a", api.registrations[0][1])
        assertEquals("token-b", api.registrations[1][1])
        assertEquals(listOf("device-a", "token-b", "1.0"), api.registrations[1])
    }

    private companion object {
        const val SERVER = "https://home.test/"
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PushRegistrationStoreNameTest {
    @Test
    fun aManuallyTypedNameIsForgottenOnLoad() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(context.filesDir, "registration-name.preferences_pb") },
        )
        dataStore.edit { it[LEGACY_NAME] = "Стас" }
        DataStorePushRegistrationStore(dataStore).load()
        assertNull(dataStore.data.first()[LEGACY_NAME])
    }

    private companion object {
        val LEGACY_NAME = stringPreferencesKey("push_device_name")
    }
}

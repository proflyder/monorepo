package dev.proflyder.home

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.network.ConnectSchedulerClient
import dev.proflyder.home.core.settings.AndroidConnectionSettingsStore
import dev.proflyder.home.core.settings.HomeDataStore
import dev.proflyder.home.core.settings.TokenCipher
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ConnectionSettingsClearTest {
    private class FakeCipher : TokenCipher {
        override fun encrypt(plaintext: String): String = "enc:$plaintext"
        override fun decrypt(encoded: String): String? = encoded.removePrefix("enc:").takeIf { encoded.startsWith("enc:") }
    }

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private val context: Context = RuntimeEnvironment.getApplication()

    @After
    fun tearDown() = scope.cancel()

    private fun dataStore() = HomeDataStore.create(context, "test-" + UUID.randomUUID(), scope)

    private fun endpoint(url: String) = (ServerEndpoint.parse(url) as EndpointParseResult.Valid).endpoint

    @Test
    fun clearingTokenRemovesCiphertextAndNetworkFailsBeforeRequest() = runBlocking {
        val dataStore = dataStore()
        val endpoint = endpoint("http://192.0.2.10:8080")
        val store = AndroidConnectionSettingsStore(dataStore, FakeCipher())

        val saved = store.update(ConnectionConfig(endpoint = endpoint, bearerToken = "valid-token"))
        assertTrue(saved is Outcome.Success)
        assertTrue(store.config.value.hasToken)
        assertTrue(dataStore.data.first().hasToken())

        val cleared = store.update(ConnectionConfig(endpoint = endpoint, bearerToken = null))
        assertTrue(cleared is Outcome.Success)
        assertFalse(store.config.value.hasToken)
        assertFalse(dataStore.data.first().hasToken())

        // A subsequent request must fail with a controlled configuration error
        // and must not reach OkHttp/server.
        val client = ConnectSchedulerClient(
            settings = store,
            baseClient = OkHttpClient(),
            serverClock = ServerClock(),
        )
        assertEquals(
            RemoteFailure.Validation(Field.TOKEN),
            client.getStatus().failureOrNull(),
        )
    }

    @Test
    fun settingsPersistAcrossStoreInstances() = runBlocking {
        val dataStore = dataStore()
        val first = AndroidConnectionSettingsStore(dataStore, FakeCipher())
        first.update(
            ConnectionConfig(
                endpoint = endpoint("http://192.0.2.10:8080"),
                controlEndpoint = endpoint("http://192.0.2.10:8099"),
                bearerToken = "valid-token",
                connectTimeoutMillis = 3_000,
                readTimeoutMillis = 9_000,
            ),
        )

        // A new store over the same DataStore (an app restart) sees the same settings.
        val second = AndroidConnectionSettingsStore(dataStore, FakeCipher())
        val config = second.config.value
        assertEquals("http://192.0.2.10:8080", config.endpoint?.baseUrl())
        assertEquals("http://192.0.2.10:8099", config.controlEndpoint?.baseUrl())
        assertEquals("valid-token", config.bearerToken)
        assertEquals(3_000L, config.connectTimeoutMillis)
        assertEquals(9_000L, config.readTimeoutMillis)
        assertEquals(TokenStatus.AVAILABLE, second.tokenStatus.value)

        // Removing the control endpoint is persisted too.
        second.update(config.copy(controlEndpoint = null))
        assertEquals(null, AndroidConnectionSettingsStore(dataStore, FakeCipher()).config.value.controlEndpoint)
    }

    @Test
    fun legacySharedPreferencesAreMigratedOnFirstOpen() = runBlocking {
        context.getSharedPreferences(HomeDataStore.LEGACY_CONNECTION_PREFS, Context.MODE_PRIVATE).edit()
            .putString("endpoint", "http://192.0.2.10:8080")
            .putString("control_endpoint", "http://192.0.2.10:8099")
            .putString("bearer_token_ciphertext", "enc:legacy-token")
            .putLong("connect_timeout_millis", 7_000L)
            .putLong("read_timeout_millis", 11_000L)
            .commit()

        val store = AndroidConnectionSettingsStore(dataStore(), FakeCipher())

        val config = store.config.value
        assertEquals("http://192.0.2.10:8080", config.endpoint?.baseUrl())
        assertEquals("http://192.0.2.10:8099", config.controlEndpoint?.baseUrl())
        assertEquals("legacy-token", config.bearerToken)
        assertEquals(7_000L, config.connectTimeoutMillis)
        assertEquals(11_000L, config.readTimeoutMillis)
        assertEquals(TokenStatus.AVAILABLE, store.tokenStatus.value)
    }

    private fun Preferences.hasToken(): Boolean =
        asMap().keys.any { it.name == "bearer_token_ciphertext" }
}

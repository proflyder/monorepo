package dev.proflyder.home

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.RemoteConnectionSource
import dev.proflyder.home.core.config.RemoteConnectionState
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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
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

    private class FakeRemoteSource(initial: RemoteConnectionState) : RemoteConnectionSource {
        private val mutable = MutableStateFlow(initial)
        override val state: StateFlow<RemoteConnectionState> = mutable

        fun emit(next: RemoteConnectionState) {
            mutable.value = next
        }

        override suspend fun refresh() = Unit
    }

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private val context: Context = RuntimeEnvironment.getApplication()

    @Before
    fun clearLegacyPreferences() {
        context.getSharedPreferences(HomeDataStore.LEGACY_CONNECTION_PREFS, Context.MODE_PRIVATE)
            .edit().clear().commit()
    }

    @After
    fun tearDown() = scope.cancel()

    private fun dataStore() = HomeDataStore.create(context, "test-" + UUID.randomUUID(), scope)

    private fun endpoint(url: String) = (ServerEndpoint.parse(url) as EndpointParseResult.Valid).endpoint

    private fun loadedRemote(
        scheduler: String = "http://192.0.2.10:8080",
        control: String? = "http://192.0.2.10:8099",
        connectMillis: Long = 5_000,
        readMillis: Long = 20_000,
    ) = RemoteConnectionState.Loaded(
        endpoint = endpoint(scheduler),
        controlEndpoint = control?.let(::endpoint),
        connectTimeoutMillis = connectMillis,
        readTimeoutMillis = readMillis,
    )

    @Test
    fun clearingTokenRemovesCiphertextAndNetworkFailsBeforeRequest() = runBlocking {
        val dataStore = dataStore()
        val store = AndroidConnectionSettingsStore(dataStore, FakeCipher(), FakeRemoteSource(loadedRemote()))

        val saved = store.updateToken("valid-token")
        assertTrue(saved is Outcome.Success)
        assertTrue(store.config.value.hasToken)
        assertEquals(loadedRemote().endpoint, store.config.value.endpoint)
        assertTrue(dataStore.data.first().hasToken())

        val cleared = store.updateToken(null)
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
    fun tokenPersistsAndEndpointsComeFromRemoteConfig() = runBlocking {
        val dataStore = dataStore()
        val remote = FakeRemoteSource(loadedRemote())
        val first = AndroidConnectionSettingsStore(dataStore, FakeCipher(), remote)
        first.updateToken("valid-token")

        // A new store over the same DataStore (an app restart) sees the token
        // and the Remote Config values, not any legacy endpoint fields.
        val second = AndroidConnectionSettingsStore(dataStore, FakeCipher(), remote)
        val config = second.config.value
        assertEquals("http://192.0.2.10:8080", config.endpoint?.baseUrl())
        assertEquals("http://192.0.2.10:8099", config.controlEndpoint?.baseUrl())
        assertEquals("valid-token", config.bearerToken)
        assertEquals(5_000L, config.connectTimeoutMillis)
        assertEquals(20_000L, config.readTimeoutMillis)
        assertEquals(TokenStatus.AVAILABLE, second.tokenStatus.value)
    }

    @Test
    fun remoteConfigChangesRecomposeTheSnapshot() = runBlocking {
        val remote = FakeRemoteSource(loadedRemote())
        val store = AndroidConnectionSettingsStore(dataStore(), FakeCipher(), remote)
        store.updateToken("valid-token")

        remote.emit(
            loadedRemote(
                scheduler = "https://alarms.example.test",
                control = null,
                connectMillis = 3_000,
                readMillis = 9_000,
            ),
        )

        val config = withTimeout(5_000) {
            store.config.first { it.endpoint?.baseUrl() == "https://alarms.example.test" }
        }
        assertEquals(null, config.controlEndpoint)
        assertEquals(3_000L, config.connectTimeoutMillis)
        assertEquals(9_000L, config.readTimeoutMillis)
        assertEquals("valid-token", config.bearerToken)
    }

    @Test
    fun unavailableRemoteConfigHasNoEndpointsButKeepsToken() = runBlocking {
        val store = AndroidConnectionSettingsStore(
            dataStore(),
            FakeCipher(),
            FakeRemoteSource(RemoteConnectionState.Unavailable),
        )
        store.updateToken("valid-token")

        val config = store.config.value
        assertEquals(null, config.endpoint)
        assertEquals(ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS, config.connectTimeoutMillis)
        assertEquals("valid-token", config.bearerToken)
    }

    @Test
    fun legacySharedPreferencesAreMigratedOnFirstOpen() = runBlocking {
        context.getSharedPreferences(HomeDataStore.LEGACY_CONNECTION_PREFS, Context.MODE_PRIVATE).edit()
            .putString("endpoint", "http://192.0.2.99:8080")
            .putString("control_endpoint", "http://192.0.2.99:8099")
            .putString("bearer_token_ciphertext", "enc:legacy-token")
            .putLong("connect_timeout_millis", 7_000L)
            .putLong("read_timeout_millis", 11_000L)
            .commit()

        val store = AndroidConnectionSettingsStore(dataStore(), FakeCipher(), FakeRemoteSource(loadedRemote()))

        val config = store.config.value
        // Endpoints/timeouts are Remote Config-authoritative; only the encrypted
        // token survives from the legacy DataStore.
        assertEquals("http://192.0.2.10:8080", config.endpoint?.baseUrl())
        assertEquals("http://192.0.2.10:8099", config.controlEndpoint?.baseUrl())
        assertEquals("legacy-token", config.bearerToken)
        assertEquals(5_000L, config.connectTimeoutMillis)
        assertEquals(20_000L, config.readTimeoutMillis)
        assertEquals(TokenStatus.AVAILABLE, store.tokenStatus.value)
    }

    private fun Preferences.hasToken(): Boolean =
        asMap().keys.any { it.name == "bearer_token_ciphertext" }
}

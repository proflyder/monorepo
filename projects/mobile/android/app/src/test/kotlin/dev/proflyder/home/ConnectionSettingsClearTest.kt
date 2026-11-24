package dev.proflyder.home

import dev.proflyder.home.core.model.DeviceIdentity
import android.content.Context
import androidx.datastore.core.DataStore
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
import dev.proflyder.home.core.runtime.AndroidClock
import dev.proflyder.home.core.network.ConnectSchedulerClient
import dev.proflyder.home.core.network.HomeHttp
import dev.proflyder.home.core.network.RouteGate
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.settings.AndroidConnectionSettingsStore
import dev.proflyder.home.core.settings.HomeDataStore
import dev.proflyder.home.core.settings.TokenCipher
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
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

    private fun dataStore() = HomeDataStore.create(context, scope, "test-" + UUID.randomUUID())

    private fun endpoint(url: String) = (ServerEndpoint.parse(url) as EndpointParseResult.Valid).endpoint

    private fun loadedRemote(
        scheduler: String = "http://192.0.2.10:8080",
        control: String? = "http://192.0.2.10:8099",
        connectMillis: Long = 5_000,
        readMillis: Long = 20_000,
    ) = RemoteConnectionState.Loaded(
        endpoint = endpoint(scheduler),
        controlEndpoint = control?.let(::endpoint),
        intercomEndpoint = null,
        connectTimeoutMillis = connectMillis,
        readTimeoutMillis = readMillis,
    )

    @Test
    fun clearingTokenRemovesCiphertextAndNetworkFailsBeforeRequest() = runBlocking {
        val dataStore = dataStore()
        val store = store(dataStore, FakeCipher(), FakeRemoteSource(loadedRemote()))

        val saved = store.updateToken("valid-token")
        assertTrue(saved is Outcome.Success)
        assertTrue(store.config.value.hasToken)
        assertEquals(loadedRemote().endpoint, store.config.value.endpoint)
        assertTrue(dataStore.data.first().hasToken())

        val cleared = store.updateToken(null)
        assertTrue(cleared is Outcome.Success)
        assertFalse(store.config.value.hasToken)
        assertFalse(dataStore.data.first().hasToken())

        val http = HomeHttp(
            baseClient = OkHttpClient(),
            settings = store,
            inspection = emptySet(),
            gate = OpenRouteGate,
            version = TestAppVersion,
            scope = scope,
            io = Dispatchers.IO,
            identity = NoIdentity,
            forbidden = { emptySet() },
        )
        val client = ConnectSchedulerClient(http, ServerClock(AndroidClock))
        assertEquals(
            RemoteFailure.Validation(Field.TOKEN),
            client.getStatus().failureOrNull(),
        )
    }

    @Test
    fun tokenPersistsAndEndpointsComeFromRemoteConfig() = runBlocking {
        val dataStore = dataStore()
        val remote = FakeRemoteSource(loadedRemote())
        val first = store(dataStore, FakeCipher(), remote)
        first.updateToken("valid-token")

        val second = store(dataStore, FakeCipher(), remote)
        val config = second.loadedConfig()
        assertEquals("http://192.0.2.10:8080", config.endpoint?.baseUrl())
        assertEquals("http://192.0.2.10:8099", config.controlEndpoint?.baseUrl())
        assertEquals("valid-token", config.bearerToken)
        assertEquals(5_000L, config.connectTimeoutMillis)
        assertEquals(20_000L, config.readTimeoutMillis)
        assertEquals(TokenStatus.AVAILABLE, withTimeout(5_000) { second.tokenStatus.first { it != TokenStatus.LOADING } })
    }

    @Test
    fun remoteConfigChangesRecomposeTheSnapshot() = runBlocking {
        val remote = FakeRemoteSource(loadedRemote())
        val store = store(dataStore(), FakeCipher(), remote)
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
        val store = store(dataStore(), FakeCipher(), FakeRemoteSource(RemoteConnectionState.Unavailable))
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

        val store = store(dataStore(), FakeCipher(), FakeRemoteSource(loadedRemote()))

        val config = store.loadedConfig()
        assertEquals("http://192.0.2.10:8080", config.endpoint?.baseUrl())
        assertEquals("http://192.0.2.10:8099", config.controlEndpoint?.baseUrl())
        assertEquals("legacy-token", config.bearerToken)
        assertEquals(5_000L, config.connectTimeoutMillis)
        assertEquals(20_000L, config.readTimeoutMillis)
        assertEquals(TokenStatus.AVAILABLE, withTimeout(5_000) { store.tokenStatus.first { it != TokenStatus.LOADING } })
    }

    @Test
    fun constructionDoesNotWaitForDiskAndReportsLoading() = runBlocking {
        val gate = CompletableDeferred<Unit>()
        val disk = dataStore()
        val slow = object : DataStore<Preferences> by disk {
            override val data: Flow<Preferences> = flow { gate.await(); emitAll(disk.data) }
        }
        val store = store(slow, FakeCipher(), FakeRemoteSource(loadedRemote()))

        assertEquals(TokenStatus.LOADING, store.tokenStatus.value)
        assertFalse(store.loaded.first())
        gate.complete(Unit)
        val status = withTimeout(5_000) { store.tokenStatus.first { it != TokenStatus.LOADING } }
        assertEquals(TokenStatus.ABSENT, status)
    }

    private fun store(
        dataStore: DataStore<Preferences>,
        cipher: TokenCipher,
        remote: RemoteConnectionSource,
    ) = AndroidConnectionSettingsStore(dataStore, cipher, remote, scope, Dispatchers.IO)

    private suspend fun AndroidConnectionSettingsStore.loadedConfig(): ConnectionConfig =
        withTimeout(5_000) { config.first { it.bearerToken != null } }

    private fun Preferences.hasToken(): Boolean =
        asMap().keys.any { it.name == "bearer_token_ciphertext" }
}

private object OpenRouteGate : RouteGate {
    override suspend fun await() = Unit
}

private object TestAppVersion : AppVersion {
    override val name: String = "test"

    override val debuggable: Boolean = false
}

private object NoIdentity : DeviceIdentity {
    override suspend fun deviceId(): String = ""

    override fun defaultName(): String = ""
}

package dev.proflyder.home.feature.push

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.PushDevicesApi
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.testing.FakeClock
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runCurrent
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class HomePushRegistrarTest {
    private val scope = TestScope(UnconfinedTestDispatcher())
    private val api = RecordingApi()
    private val store = MemoryStore()
    private val tokens = MutableTokenSource()
    private val settings = MutableSettings()
    private val activation = HomeActivation()
    private val registrar = HomePushRegistrar(
        store = store,
        tokens = tokens,
        publisher = PushRegistrationPublisher(api, FixedIdentity),
        identity = FixedIdentity,
        settings = settings,
        version = FixedVersion,
        clock = FakeClock(wallMillis = 1_000_000L),
        activation = activation,
        scope = scope.backgroundScope,
    )

    private class HomeActivation(active: Boolean = true) : AppActivation {
        override val active = MutableStateFlow(active)

        override fun activate() {
            this.active.value = true
        }

        override suspend fun awaitActive() {
            this.active.first { it }
        }
    }

    @After
    fun tearDown() {
        scope.cancel()
    }

    @Test
    fun nothingIsSentUntilTheServerAndTokenAreKnown() {
        registrar.start()
        scope.runCurrent()
        assertTrue(api.tokens.isEmpty())
        tokens.refreshed("token-a")
        scope.runCurrent()
        assertTrue(api.tokens.isEmpty())
    }

    @Test
    fun aColdStartDoesNotRegisterUntilTheAppIsUsed() {
        activation.active.value = false
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        registrar.start()
        scope.runCurrent()
        assertTrue(api.tokens.isEmpty())
        activation.activate()
        scope.runCurrent()
        assertEquals(listOf("token-a"), api.tokens)
    }

    @Test
    fun configuringTheServerLaterRegistersWithoutARestart() {
        tokens.refreshed("token-a")
        registrar.start()
        settings.configure("https://home.test")
        scope.runCurrent()
        assertEquals(listOf("token-a"), api.tokens)
        assertEquals("https://home.test", store.registration.value.server)
    }

    @Test
    fun aRotatedTokenOrANewServerRegistersAgain() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        registrar.start()
        scope.runCurrent()
        tokens.refreshed("token-b")
        scope.runCurrent()
        settings.configure("https://other.test")
        scope.runCurrent()
        assertEquals(listOf("token-a", "token-b", "token-b"), api.tokens)
        assertEquals("https://other.test", store.registration.value.server)
    }

    @Test
    fun aFailedRegistrationIsReportedAndBusyClearsAfterAnError() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        api.result = { error("network down") }
        registrar.start()
        scope.runCurrent()
        assertEquals(PushSyncStatus(busy = false, failed = true), registrar.status.value)
    }

    @Test
    fun busyIsTrueOnlyWhileTheRequestIsInFlight() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        val gate = CompletableDeferred<Outcome<Unit>>()
        api.suspended = gate
        registrar.start()
        scope.runCurrent()
        assertTrue(registrar.status.value.busy)
        gate.complete(Outcome.Success(Unit))
        scope.runCurrent()
        assertEquals(PushSyncStatus(busy = false, failed = false), registrar.status.value)
    }

    @Test
    fun renamingRegistersTheNewNameOnce() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        registrar.start()
        scope.runCurrent()
        var saved: PushRegistrationResult? = null
        scope.backgroundScope.launch { saved = registrar.saveDeviceName("  Кухня  ") }
        scope.runCurrent()
        assertEquals(PushRegistrationResult.Done, saved)
        assertEquals(listOf("Pixel", "Кухня"), api.names)
        assertEquals("Кухня", store.registration.value.registeredName)
    }

    @Test
    fun registerNowFetchesAMissingPushTokenBeforeGivingUp() {
        settings.configure("https://home.test")
        tokens.fetchable = "token-late"
        val result = registerNow()
        assertEquals(PushRegistrationResult.Done, result)
        assertEquals(listOf("token-late"), api.tokens)
    }

    @Test
    fun registerNowWithoutAPushTokenSaysSoAndSendsNothing() {
        settings.configure("https://home.test")
        val result = registerNow()
        assertEquals(PushRegistrationResult.NoPushToken, result)
        assertTrue(api.tokens.isEmpty())
        assertTrue(registrar.status.value.failed)
    }

    @Test
    fun registerNowWithoutAServerSaysItIsNotConfigured() {
        tokens.refreshed("token-a")
        assertEquals(PushRegistrationResult.NotConfigured, registerNow())
        assertTrue(api.tokens.isEmpty())
    }

    @Test
    fun registerNowReportsTheServerRejection() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        api.result = { Outcome.Failure(RemoteFailure.Unauthorized) }
        assertEquals(PushRegistrationResult.Rejected(RemoteFailure.Unauthorized), registerNow())
    }

    @Test
    fun registerNowTurnsAnUnexpectedErrorIntoAProtocolFailure() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        api.result = { error("boom") }
        val result = registerNow()
        assertTrue((result as PushRegistrationResult.Rejected).failure is RemoteFailure.ProtocolFailure)
    }

    private fun registerNow(): PushRegistrationResult? {
        var result: PushRegistrationResult? = null
        scope.backgroundScope.launch { result = registrar.registerNow() }
        scope.runCurrent()
        return result
    }

    @Test
    fun aRevokedTokenIsDroppedAndUnregistered() {
        settings.configure("https://home.test")
        tokens.refreshed("token-a")
        registrar.start()
        scope.runCurrent()
        tokens.revoked("token-a")
        registrar.unregister("token-a")
        scope.runCurrent()
        assertEquals(listOf("token-a"), api.unregistered)
        assertNull(tokens.token.value)
    }

    private class RecordingApi : PushDevicesApi {
        val tokens = mutableListOf<String>()
        val names = mutableListOf<String>()
        val unregistered = mutableListOf<String>()
        var result: () -> Outcome<Unit> = { Outcome.Success(Unit) }
        var suspended: CompletableDeferred<Outcome<Unit>>? = null

        override suspend fun register(
            deviceId: String,
            token: String,
            appVersion: String,
            deviceName: String,
        ): Outcome<Unit> {
            tokens += token
            names += deviceName
            return suspended?.await() ?: result()
        }

        override suspend fun rename(deviceId: String, deviceName: String): Outcome<Unit> = error("unused")

        override suspend fun unregister(token: String): Outcome<Unit> {
            unregistered += token
            return Outcome.Success(Unit)
        }
    }

    private class MemoryStore : PushRegistrationStore {
        private val state = MutableStateFlow(PushRegistration())
        override val registration: StateFlow<PushRegistration> = state.asStateFlow()

        override suspend fun load() = Unit

        override suspend fun saved(record: PushRegistrationRecord) {
            state.value = state.value.copy(
                token = record.token,
                registeredAtMillis = record.registeredAtMillis,
                registeredName = record.deviceName,
                appVersion = record.appVersion,
                server = record.server,
            )
        }

        override suspend fun savedName(name: String) {
            state.value = state.value.copy(deviceName = name)
        }
    }

    private class MutableTokenSource : PushTokenSource {
        private val current = MutableStateFlow<String?>(null)
        override val token: StateFlow<String?> = current.asStateFlow()
        var fetchable: String? = null

        override suspend fun fetch(): String? = fetchable?.also { current.value = it } ?: current.value

        override fun refreshed(token: String) {
            current.value = token
        }

        override fun revoked(token: String) {
            current.compareAndSet(token, null)
        }
    }

    private class MutableSettings : ConnectionSettingsStore {
        private val state = MutableStateFlow(ConnectionConfig())
        override val config: StateFlow<ConnectionConfig> = state.asStateFlow()
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.entries.first())

        fun configure(url: String) {
            val endpoint = (ServerEndpoint.parse(url) as EndpointParseResult.Valid).endpoint
            state.value = ConnectionConfig(endpoint = endpoint, bearerToken = TOKEN)
        }

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }

    private object FixedIdentity : DeviceIdentity {
        override suspend fun deviceId(): String = "device-a"

        override fun defaultName(): String = "Pixel"
    }

    private object FixedVersion : AppVersion {
        override val name: String = "1.0"

        override val debuggable: Boolean = false
    }

    private companion object {
        const val TOKEN = "0123456789abcdef0123456789abcdef"
    }
}

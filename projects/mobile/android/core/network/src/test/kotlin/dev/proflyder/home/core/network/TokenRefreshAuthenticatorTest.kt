package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

/** A rotated token is retried once; an unchanged token is not retried. */
class TokenRefreshAuthenticatorTest {
    private lateinit var server: MockWebServer
    private lateinit var store: FakeStore

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
        store = FakeStore("old-token")
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun client(onUnauthorized: () -> Unit = {}): OkHttpClient = OkHttpClient.Builder()
        .authenticator(TokenRefreshAuthenticator(store))
        .addNetworkInterceptor { chain ->
            val response = chain.proceed(chain.request())
            if (response.code == 401) onUnauthorized()
            response
        }
        .build()

    private fun request(): Request = Request.Builder()
        .url(server.url("/rpc"))
        .header("Authorization", "Bearer old-token")
        .build()

    @Test
    fun rotatedTokenIsRetriedOnceWithTheNewValue() {
        server.enqueue(MockResponse().setResponseCode(401))
        server.enqueue(MockResponse().setResponseCode(200).setBody("{}"))
        val response = client { store.set("new-token") }.newCall(request()).execute()
        assertEquals(200, response.code)
        assertEquals(2, server.requestCount)
        assertEquals("Bearer old-token", server.takeRequest().getHeader("Authorization"))
        assertEquals("Bearer new-token", server.takeRequest().getHeader("Authorization"))
    }

    @Test
    fun unchangedTokenIsNotRetried() {
        server.enqueue(MockResponse().setResponseCode(401))
        val response = client().newCall(request()).execute()
        assertEquals(401, response.code)
        assertEquals(1, server.requestCount)
    }

    private class FakeStore(initial: String) : ConnectionSettingsStore {
        private val mutable = MutableStateFlow(ConnectionConfig(bearerToken = initial))
        override val config: StateFlow<ConnectionConfig> = mutable
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.AVAILABLE)

        fun set(token: String) {
            mutable.value = mutable.value.copy(bearerToken = token)
        }

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }
}

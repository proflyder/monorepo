package dev.proflyder.home

import android.content.Context
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.network.ConnectSchedulerClient
import dev.proflyder.home.core.settings.AndroidConnectionSettingsStore
import dev.proflyder.home.core.settings.TokenCipher
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
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

    @Test
    fun clearingTokenRemovesCiphertextAndNetworkFailsBeforeRequest() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val prefs = context.getSharedPreferences("home_connection", Context.MODE_PRIVATE)
        prefs.edit().clear().commit()
        val endpoint = (ServerEndpoint.parse("http://192.0.2.10:8080") as EndpointParseResult.Valid).endpoint
        val store = AndroidConnectionSettingsStore(context, FakeCipher())

        val saved = store.update(
            ConnectionConfig(endpoint = endpoint, bearerToken = "valid-token"),
        )
        assertTrue(saved is Outcome.Success)
        assertTrue(store.config.value.hasToken)
        assertTrue(prefs.contains("bearer_token_ciphertext"))

        val cleared = store.update(ConnectionConfig(endpoint = endpoint, bearerToken = null))
        assertTrue(cleared is Outcome.Success)
        assertFalse(store.config.value.hasToken)
        assertFalse(prefs.contains("bearer_token_ciphertext"))

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
}

package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.testing.TestFixtures
import kotlinx.serialization.Serializable
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import retrofit2.Retrofit
import retrofit2.converter.kotlinx.serialization.asConverterFactory
import retrofit2.http.Body
import retrofit2.http.POST

/** The call adapter turns every HTTP/transport/decoding outcome into `Outcome`. */
class OutcomeCallAdapterTest {
    private lateinit var server: MockWebServer

    @Serializable
    private data class ReplyDto(val value: String = "")

    private interface TestApi {
        @POST("rpc")
        fun call(@Body body: EmptyBody): Outcome<ReplyDto>
    }

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun api(): TestApi = Retrofit.Builder()
        .baseUrl(server.url("/"))
        .client(OkHttpClient())
        .addConverterFactory(WireJson.asConverterFactory("application/json".toMediaType()))
        .addCallAdapterFactory(OutcomeCallAdapterFactory(WireJson))
        .build()
        .create(TestApi::class.java)

    @Test
    fun successBodyBecomesOutcomeSuccess() {
        server.enqueue(MockResponse().setResponseCode(200).setBody(TestFixtures.read("dto_reply_ok.json")))
        val result = api().call(EmptyBody)
        assertEquals(Outcome.Success(ReplyDto("ok")), result)
    }

    @Test
    fun connectErrorWithLocalizedDetailBecomesServerFailure() {
        server.enqueue(
            MockResponse().setResponseCode(412)
                .setBody(TestFixtures.read("connect_error_localized.json")),
        )
        val failure = (api().call(EmptyBody) as Outcome.Failure).failure
        failure as RemoteFailure.Server
        assertEquals("failed_precondition", failure.code)
        assertEquals("scheduler.error.schedule_not_ringing", failure.textKey)
    }

    @Test
    fun malformedBodyBecomesProtocolFailure() {
        server.enqueue(MockResponse().setResponseCode(200).setBody("{not-json"))
        assertTrue((api().call(EmptyBody) as Outcome.Failure).failure is RemoteFailure.ProtocolFailure)
    }

    @Test
    fun transportFailureBecomesOffline() {
        val dead = MockWebServer().apply { start() }
        val url = dead.url("/")
        dead.shutdown()
        val deadApi = Retrofit.Builder()
            .baseUrl(url)
            .client(OkHttpClient())
            .addConverterFactory(WireJson.asConverterFactory("application/json".toMediaType()))
            .addCallAdapterFactory(OutcomeCallAdapterFactory(WireJson))
            .build()
            .create(TestApi::class.java)
        assertTrue((deadApi.call(EmptyBody) as Outcome.Failure).failure is RemoteFailure.Offline)
    }
}

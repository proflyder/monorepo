package dev.proflyder.home.core.ui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** Server keys resolve through `strings.xml`, with explicit argument order. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ServerTextTest {
    private val context = RuntimeEnvironment.getApplication()
    private val json = Json { ignoreUnknownKeys = true }

    private fun fixture(): JsonObject {
        val text = checkNotNull(javaClass.getResource("/fixtures/connect_error.json")).readText()
        return json.parseToJsonElement(text).jsonObject
    }

    @Test
    fun keyFromAConnectErrorDetailResolvesToTheRussianOrEnglishResource() {
        val details = fixture().getValue("details") as JsonArray
        val debug = details.first().jsonObject.getValue("debug").jsonObject
        val key = debug.getValue("key").toString().trim('"')
        val resolved = serverText(context, key, emptyMap())
        assertEquals(context.getString(R.string.server_scheduler_schedule_not_ringing), resolved)
    }

    @Test
    fun typedArgumentsAreSubstitutedInDeclarationOrder() {
        val resolved = serverText(
            context,
            "scheduler.error.timer_limit",
            mapOf("limit" to "64"),
        )
        assertEquals(context.getString(R.string.server_scheduler_timer_limit, "64"), resolved)
    }

    @Test
    fun unknownKeyReturnsNullSoTheCallerCanFallBackToLegacyText() {
        assertNull(serverText(context, "scheduler.error.unknown_key", emptyMap()))
    }
}

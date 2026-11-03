package dev.proflyder.home.core.config

import dev.proflyder.home.core.model.Field
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteConnectionSourceTest {
    @Test
    fun emptyParameterIsNotConfigured() {
        assertEquals(RemoteConnectionState.Missing, parseRemoteConnection(""))
        assertEquals(RemoteConnectionState.Missing, parseRemoteConnection("   "))
    }

    @Test
    fun validDocumentLoadsAllFields() {
        val state = parseRemoteConnection(
            """
            {
              "schedulerBaseUrl": "https://alarms.example.test",
              "controlBaseUrl": "http://192.0.2.28:8099/",
              "connectTimeoutMs": 3000,
              "readTimeoutMs": 9000
            }
            """.trimIndent(),
        )

        val loaded = state as RemoteConnectionState.Loaded
        assertEquals("https://alarms.example.test", loaded.endpoint.baseUrl())
        assertEquals("http://192.0.2.28:8099", loaded.controlEndpoint?.baseUrl())
        assertEquals(3_000L, loaded.connectTimeoutMillis)
        assertEquals(9_000L, loaded.readTimeoutMillis)
    }

    @Test
    fun minimalDocumentUsesDefaultsAndDisablesHome() {
        val loaded = parseRemoteConnection("""{"schedulerBaseUrl":"http://192.0.2.10:30098"}""")
            as RemoteConnectionState.Loaded

        assertNull(loaded.controlEndpoint)
        assertEquals(ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS, loaded.connectTimeoutMillis)
        assertEquals(ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS, loaded.readTimeoutMillis)
    }

    @Test
    fun blankControlAddressDisablesHomeTab() {
        val loaded = parseRemoteConnection(
            """{"schedulerBaseUrl":"http://192.0.2.10:30098","controlBaseUrl":""}""",
        ) as RemoteConnectionState.Loaded

        assertNull(loaded.controlEndpoint)
    }

    @Test
    fun malformedJsonIsInvalidWithoutThrowing() {
        val state = parseRemoteConnection("""{"schedulerBaseUrl":""")
        assertTrue(state is RemoteConnectionState.Invalid)
        assertNull((state as RemoteConnectionState.Invalid).field)
    }

    @Test
    fun missingRequiredSchedulerAddressIsInvalid() {
        val state = parseRemoteConnection("""{"controlBaseUrl":"http://192.0.2.10:8099"}""")
        assertEquals(Field.ENDPOINT, (state as RemoteConnectionState.Invalid).field)

        val blank = parseRemoteConnection("""{"schedulerBaseUrl":"   "}""")
        assertEquals(Field.ENDPOINT, (blank as RemoteConnectionState.Invalid).field)
    }

    @Test
    fun invalidSchedulerAddressIsReported() {
        val state = parseRemoteConnection("""{"schedulerBaseUrl":"192.0.2.10:30098"}""")
        assertEquals(Field.ENDPOINT, (state as RemoteConnectionState.Invalid).field)
    }

    @Test
    fun invalidControlAddressIsReported() {
        val state = parseRemoteConnection(
            """{"schedulerBaseUrl":"http://192.0.2.10:30098","controlBaseUrl":"not a url"}""",
        )
        assertEquals(Field.CONTROL_ENDPOINT, (state as RemoteConnectionState.Invalid).field)
    }

    @Test
    fun outOfRangeTimeoutsAreInvalid() {
        val below = parseRemoteConnection(
            """{"schedulerBaseUrl":"http://192.0.2.10:30098","connectTimeoutMs":${ConnectionConfig.MIN_TIMEOUT_MILLIS - 1}}""",
        )
        val above = parseRemoteConnection(
            """{"schedulerBaseUrl":"http://192.0.2.10:30098","readTimeoutMs":${ConnectionConfig.MAX_TIMEOUT_MILLIS + 1}}""",
        )

        assertEquals(Field.TIMEOUT, (below as RemoteConnectionState.Invalid).field)
        assertEquals(Field.TIMEOUT, (above as RemoteConnectionState.Invalid).field)
    }

    @Test
    fun nonNumericTimeoutIsInvalid() {
        val state = parseRemoteConnection(
            """{"schedulerBaseUrl":"http://192.0.2.10:30098","connectTimeoutMs":"soon"}""",
        )
        assertEquals(Field.TIMEOUT, (state as RemoteConnectionState.Invalid).field)
    }

    @Test
    fun timeoutBoundariesAreAccepted() {
        val state = parseRemoteConnection(
            """
            {
              "schedulerBaseUrl": "http://192.0.2.10:30098",
              "connectTimeoutMs": ${ConnectionConfig.MIN_TIMEOUT_MILLIS},
              "readTimeoutMs": ${ConnectionConfig.MAX_TIMEOUT_MILLIS}
            }
            """.trimIndent(),
        )
        assertTrue(state is RemoteConnectionState.Loaded)
    }

    @Test
    fun unknownKeysAreIgnored() {
        val loaded = parseRemoteConnection(
            """
            {
              "schedulerBaseUrl": "http://192.0.2.10:30098",
              "futureOption": true,
              "nested": {"ignored": [1, 2, 3]}
            }
            """.trimIndent(),
        ) as RemoteConnectionState.Loaded

        assertEquals("http://192.0.2.10:30098", loaded.endpoint.baseUrl())
    }
}

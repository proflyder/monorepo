package dev.proflyder.home.core.endpoint

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ServerEndpointTest {
    private fun valid(raw: String): ServerEndpoint =
        (ServerEndpoint.parse(raw) as EndpointParseResult.Valid).endpoint

    private fun problem(raw: String): EndpointProblem =
        (ServerEndpoint.parse(raw) as EndpointParseResult.Invalid).problem

    @Test
    fun acceptsHttpsDnsNameWithDefaultPort() {
        val endpoint = valid("https://home.tailnet.ts.net")
        assertEquals(ServerEndpoint.Scheme.HTTPS, endpoint.scheme)
        assertEquals("home.tailnet.ts.net", endpoint.host)
        assertEquals(443, endpoint.port)
        assertFalse(endpoint.portExplicit)
        assertEquals("https://home.tailnet.ts.net", endpoint.baseUrl())
        assertEquals(
            "https://home.tailnet.ts.net/ricardo.v1.SchedulerService/GetStatus",
            endpoint.rpcUrl("ricardo.v1.SchedulerService", "GetStatus"),
        )
    }

    @Test
    fun acceptsLanHttpWithPortAndStripsTrailingSlash() {
        val endpoint = valid("http://192.168.1.10:8080/")
        assertEquals(ServerEndpoint.Scheme.HTTP, endpoint.scheme)
        assertEquals(8080, endpoint.port)
        assertTrue(endpoint.portExplicit)
        assertEquals("http://192.168.1.10:8080", endpoint.baseUrl())
    }

    @Test
    fun acceptsBracketedIpv6AndKeepsBracketsInUrl() {
        val endpoint = valid("http://[fd7a:115c:a1e0::1]:8080")
        assertEquals("fd7a:115c:a1e0::1", endpoint.host)
        assertEquals("http://[fd7a:115c:a1e0::1]:8080", endpoint.baseUrl())
        assertEquals("[fd7a:115c:a1e0::1]:8080", endpoint.authority)
    }

    @Test
    fun normalizesCaseAndOptionalPrefix() {
        val endpoint = valid("HTTPS://Home.Example.TS.NET:443/base")
        assertEquals("home.example.ts.net", endpoint.host)
        assertEquals("/base", endpoint.basePath)
        assertEquals("https://home.example.ts.net/base", endpoint.baseUrl())
    }

    @Test
    fun dropsDefaultPortFromCanonicalUrl() {
        val endpoint = valid("http://home.local:80")
        assertEquals(80, endpoint.port)
        assertEquals("http://home.local", endpoint.baseUrl())
    }

    @Test
    fun rejectsMissingSchemeInsteadOfGuessing() {
        assertEquals(EndpointProblem.MISSING_SCHEME, problem("home.tailnet.ts.net"))
    }

    @Test
    fun rejectsUnsupportedScheme() {
        assertEquals(EndpointProblem.UNSUPPORTED_SCHEME, problem("ftp://home.tailnet.ts.net"))
        assertEquals(EndpointProblem.UNSUPPORTED_SCHEME, problem("ws://home.tailnet.ts.net"))
    }

    @Test
    fun rejectsCredentialsInUrl() {
        assertEquals(
            EndpointProblem.USERINFO_NOT_ALLOWED,
            problem("https://user:secret@home.tailnet.ts.net"),
        )
    }

    @Test
    fun rejectsQueryBecauseItOftenCarriesTokens() {
        assertEquals(
            EndpointProblem.QUERY_NOT_ALLOWED,
            problem("https://home.tailnet.ts.net/?token=secret"),
        )
    }

    @Test
    fun rejectsFragment() {
        assertEquals(
            EndpointProblem.FRAGMENT_NOT_ALLOWED,
            problem("https://home.tailnet.ts.net/path#token"),
        )
    }

    @Test
    fun rejectsDotSegmentsAndEncodedPaths() {
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://home.tailnet.ts.net/a/../b"))
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://home.tailnet.ts.net/%2e%2e/admin"))
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://home.tailnet.ts.net/a//b"))
    }

    @Test
    fun rejectsInvalidPorts() {
        assertEquals(EndpointProblem.INVALID_PORT, problem("https://home.tailnet.ts.net:0"))
        assertEquals(EndpointProblem.INVALID_PORT, problem("https://home.tailnet.ts.net:70000"))
    }

    @Test
    fun rejectsTraversalEvenWithTrailingSlash() {
        assertEquals(EndpointProblem.INVALID_PATH, problem("http://host/../"))
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://host/prefix/../"))
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://host/%2e%2e/"))
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://host/prefix/.."))
        assertEquals(EndpointProblem.INVALID_PATH, problem("https://host/prefix//"))
    }

    @Test
    fun acceptsSingleTrailingSlashOnPrefix() {
        val endpoint = valid("https://host.example/prefix/")
        assertEquals("/prefix", endpoint.basePath)
        assertEquals("https://host.example/prefix", endpoint.baseUrl())
    }

    @Test
    fun rejectsWhitespaceAndControls() {
        assertEquals(EndpointProblem.WHITESPACE, problem("https://home.tailnet.ts.net /x"))
        assertEquals(EndpointProblem.WHITESPACE, problem("https://home.tailnet.ts.net\n/x"))
    }

    @Test
    fun rejectsInvalidHostsAndIpv4Octets() {
        assertEquals(EndpointProblem.INVALID_HOST, problem("http://-bad.example"))
        assertEquals(EndpointProblem.INVALID_HOST, problem("http://bad_.example"))
        assertEquals(EndpointProblem.INVALID_HOST, problem("http://999.1.1.1"))
        assertEquals(EndpointProblem.INVALID_HOST, problem("http://01.2.3.4"))
    }

    @Test
    fun rejectsTooLongAndEmpty() {
        assertEquals(EndpointProblem.EMPTY, problem("   "))
        val tooLong = "https://" + "a".repeat(ServerEndpoint.MAX_LENGTH)
        assertEquals(EndpointProblem.TOO_LONG, problem(tooLong))
    }

    @Test
    fun toStringNeverIncludesCredentials() {
        val endpoint = valid("https://home.tailnet.ts.net")
        val text = endpoint.toString()
        assertFalse(text.contains("@"))
        assertFalse(text.contains("token", ignoreCase = true))
    }

    @Test
    fun acceptsMagicDnsNameAndLocalhost() {
        assertEquals("home-server.tail1234.ts.net", valid("http://home-server.tail1234.ts.net").host)
        assertEquals("localhost", valid("http://localhost:8080").host)
    }
}

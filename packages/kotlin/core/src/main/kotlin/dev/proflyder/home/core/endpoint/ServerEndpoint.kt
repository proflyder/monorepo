package dev.proflyder.home.core.endpoint

import java.net.URI
import java.net.URISyntaxException
import java.util.Locale

/**
 * A validated base endpoint for the home server's Connect listener.
 *
 * The client never accepts credentials, query parameters, fragments, or
 * ambiguous schemes. The bearer token is a separate secret and is never part
 * of a URL; [toString] is therefore safe for diagnostics.
 */
class ServerEndpoint private constructor(
    val scheme: Scheme,
    val host: String,
    val port: Int,
    val portExplicit: Boolean,
    val basePath: String,
) {
    enum class Scheme {
        HTTP,
        HTTPS,
        ;

        val text: String get() = name.lowercase(Locale.ROOT)
    }

    val isSecure: Boolean get() = scheme == Scheme.HTTPS

    /** `host:port` with IPv6 literals bracketed, suitable for a Host header. */
    val authority: String
        get() {
            val hostPart = if (host.contains(':')) "[$host]" else host
            return if (portExplicit) "$hostPart:$port" else hostPart
        }

    /** Canonical scheme://authority[/prefix] with no trailing slash. */
    fun baseUrl(): String {
        val hostPart = if (host.contains(':')) "[$host]" else host
        val defaultPort = if (isSecure) 443 else 80
        val portPart = if (port != defaultPort) ":$port" else ""
        return "${scheme.text}://$hostPart$portPart$basePath"
    }

    /** URL for a Connect unary call below [basePath]. */
    fun rpcUrl(service: String, method: String): String {
        require(service.isNotBlank() && method.isNotBlank()) { "service and method are required" }
        return "${baseUrl()}/$service/$method"
    }

    override fun toString(): String = baseUrl()

    override fun equals(other: Any?): Boolean =
        other is ServerEndpoint &&
            scheme == other.scheme &&
            host == other.host &&
            port == other.port &&
            portExplicit == other.portExplicit &&
            basePath == other.basePath

    override fun hashCode(): Int {
        var result = scheme.hashCode()
        result = 31 * result + host.hashCode()
        result = 31 * result + port
        result = 31 * result + portExplicit.hashCode()
        result = 31 * result + basePath.hashCode()
        return result
    }

    companion object {
        const val MAX_LENGTH = 512

        /**
         * Parses a user-entered LAN address or home HTTPS base URL.
         *
         * Accepted forms: `http://host[:port][/prefix]`,
         * `https://host[:port][/prefix]`, IPv4, bracketed IPv6, and DNS names.
         * A bare host is rejected so the transport is never guessed.
         */
        fun parse(raw: String): EndpointParseResult {
            val value = raw.trim()
            if (value.isEmpty()) return EndpointParseResult.Invalid(EndpointProblem.EMPTY)
            if (value.length > MAX_LENGTH) return EndpointParseResult.Invalid(EndpointProblem.TOO_LONG)
            if (value.any { it.isWhitespace() || it.isISOControl() }) {
                return EndpointParseResult.Invalid(EndpointProblem.WHITESPACE)
            }
            if (!value.contains("://")) {
                return EndpointParseResult.Invalid(EndpointProblem.MISSING_SCHEME)
            }

            val uri = try {
                URI(value)
            } catch (_: URISyntaxException) {
                return EndpointParseResult.Invalid(EndpointProblem.MALFORMED)
            }

            val scheme = when (uri.scheme?.lowercase(Locale.ROOT)) {
                "http" -> Scheme.HTTP
                "https" -> Scheme.HTTPS
                null -> return EndpointParseResult.Invalid(EndpointProblem.MISSING_SCHEME)
                else -> return EndpointParseResult.Invalid(EndpointProblem.UNSUPPORTED_SCHEME)
            }

            if (uri.rawUserInfo != null) {
                return EndpointParseResult.Invalid(EndpointProblem.USERINFO_NOT_ALLOWED)
            }
            if (uri.rawQuery != null) {
                // Query strings are rejected because they commonly carry
                // tokens or debug switches; the token belongs in a header.
                return EndpointParseResult.Invalid(EndpointProblem.QUERY_NOT_ALLOWED)
            }
            if (uri.rawFragment != null) {
                return EndpointParseResult.Invalid(EndpointProblem.FRAGMENT_NOT_ALLOWED)
            }

            val rawHost = uri.host ?: return EndpointParseResult.Invalid(
                if (uri.rawAuthority.isNullOrBlank()) EndpointProblem.MISSING_HOST
                else EndpointProblem.INVALID_HOST,
            )
            val normalizedHost = normalizeHost(rawHost)
                ?: return EndpointParseResult.Invalid(EndpointProblem.INVALID_HOST)

            val explicitPort = uri.port != -1
            val port = when {
                explicitPort -> uri.port
                scheme == Scheme.HTTPS -> 443
                else -> 80
            }
            if (port !in 1..65535) {
                return EndpointParseResult.Invalid(EndpointProblem.INVALID_PORT)
            }

            val path = normalizeBasePath(uri.rawPath)
                ?: return EndpointParseResult.Invalid(EndpointProblem.INVALID_PATH)

            return EndpointParseResult.Valid(
                ServerEndpoint(
                    scheme = scheme,
                    host = normalizedHost,
                    port = port,
                    portExplicit = explicitPort,
                    basePath = path,
                ),
            )
        }

        private fun normalizeHost(raw: String): String? {
            var host = raw
            if (host.startsWith("[") && host.endsWith("]")) {
                host = host.substring(1, host.length - 1)
            }
            if (host.isEmpty()) return null
            host = host.lowercase(Locale.ROOT)
            if (host.endsWith(".")) host = host.dropLast(1)
            if (host.isEmpty() || host.length > 253) return null

            if (host.contains(':')) {
                // IPv6 literal. URI has already validated the bracket syntax,
                // but reject zone ids and embedded characters that could be
                // confused with a hostname or authority delimiter.
                if (!host.matches(Regex("^[0-9a-f:.]+$"))) return null
                if (host.contains("%")) return null
                if (!host.contains(":")) return null
                return host
            }

            if (host.all { it.isDigit() || it == '.' }) {
                return if (isValidIpv4(host)) host else null
            }

            val labels = host.split('.')
            if (labels.any { !isValidDnsLabel(it) }) return null
            return host
        }

        private fun isValidDnsLabel(label: String): Boolean {
            if (label.isEmpty() || label.length > 63) return false
            if (label.startsWith("-") || label.endsWith("-")) return false
            return label.all { it.isLetterOrDigit() && it.code < 128 || it == '-' }
        }

        private fun isValidIpv4(host: String): Boolean {
            val parts = host.split('.')
            if (parts.size != 4) return false
            return parts.all { part ->
                part.isNotEmpty() && part.length <= 3 && part.all { it.isDigit() } &&
                    part.toIntOrNull()?.let { it in 0..255 } == true &&
                    (part == "0" || !part.startsWith("0"))
            }
        }

        private fun normalizeBasePath(rawPath: String?): String? {
            if (rawPath.isNullOrEmpty() || rawPath == "/") return ""
            if (!rawPath.startsWith("/")) return null

            // Strip exactly one trailing slash first, then validate the whole
            // remaining path. Returning early here previously skipped traversal
            // and encoded-path checks for inputs such as `/../` and `/%2e%2e/`.
            val path = if (rawPath.endsWith("/")) rawPath.dropLast(1) else rawPath
            if (path.isEmpty()) return ""
            if (path == "/") return null // input was "//"
            if (path.contains("//")) return null
            if (path.contains('\\')) return null
            if (path.contains('%')) return null
            val segments = path.split('/').drop(1)
            if (segments.any { it.isEmpty() || it == "." || it == ".." }) return null
            return path
        }
    }
}

sealed interface EndpointParseResult {
    data class Valid(val endpoint: ServerEndpoint) : EndpointParseResult
    data class Invalid(val problem: EndpointProblem) : EndpointParseResult
}

enum class EndpointProblem {
    EMPTY,
    TOO_LONG,
    WHITESPACE,
    MISSING_SCHEME,
    UNSUPPORTED_SCHEME,
    MALFORMED,
    MISSING_HOST,
    INVALID_HOST,
    INVALID_PORT,
    USERINFO_NOT_ALLOWED,
    QUERY_NOT_ALLOWED,
    FRAGMENT_NOT_ALLOWED,
    INVALID_PATH,
}

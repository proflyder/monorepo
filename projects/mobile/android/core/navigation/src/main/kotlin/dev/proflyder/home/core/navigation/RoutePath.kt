package dev.proflyder.home.core.navigation

@JvmInline
value class RoutePath private constructor(val value: String) {
    val segments: List<String> get() = value.split(SEPARATOR)

    fun tail(vararg prefix: String): String? {
        val parts = segments
        if (parts.size != prefix.size + 1 || parts.take(prefix.size) != prefix.toList()) return null
        return parts.last().takeIf { it.isNotBlank() }
    }

    companion object {
        private const val SEPARATOR = '/'

        fun of(raw: String): RoutePath? = raw.trim().trim(SEPARATOR).takeIf { it.isNotEmpty() }?.let(::RoutePath)
    }
}

fun interface RouteCodec<D : Destination> {
    fun decode(path: RoutePath): D?

    companion object {
        fun <D : Destination> fixed(destination: D): RouteCodec<D> =
            RouteCodec { path -> destination.takeIf { path.value == destination.route } }
    }
}

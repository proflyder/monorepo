package dev.proflyder.home.core.testing

/**
 * Loads a JSON payload from the test classpath (`src/test/resources/fixtures`).
 *
 * Mock server responses live in files instead of Kotlin string literals so a
 * fixture can be read, diffed and validated by tooling. Every test jar that
 * uses this helper declares its own `src/test/resources` as Bazel `resources`.
 */
object TestFixtures {
    fun read(name: String): String {
        val stream = TestFixtures::class.java.getResourceAsStream("/fixtures/$name")
            ?: error("Missing test fixture /fixtures/$name")
        return stream.use { it.readBytes().decodeToString() }.trimEnd()
    }
}

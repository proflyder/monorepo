package dev.proflyder.home.core.testing

object TestFixtures {
    fun read(name: String): String {
        val stream = TestFixtures::class.java.getResourceAsStream("/fixtures/$name")
            ?: error("Missing test fixture /fixtures/$name")
        return stream.use { it.readBytes().decodeToString() }.trimEnd()
    }
}

package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoTryTest {
    @Test
    fun `reports try catch`() {
        val code = """
            fun read(): String = try {
                "value"
            } catch (e: Exception) {
                "fallback"
            }
        """.trimIndent()

        assertEquals(1, NoTry().lint(code).size)
    }

    @Test
    fun `reports try finally`() {
        val code = """
            fun write() {
                try {
                    println("value")
                } finally {
                    println("done")
                }
            }
        """.trimIndent()

        assertEquals(1, NoTry().lint(code).size)
    }

    @Test
    fun `accepts runCatching`() {
        val code = """
            fun read(): String = runCatching { "value" }.getOrElse { "fallback" }
        """.trimIndent()

        assertEquals(0, NoTry().lint(code).size)
    }
}

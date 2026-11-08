package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoElseIfTest {
    @Test
    fun `reports an else-if chain`() {
        val code = """
            fun label(value: Int) = if (value == 1) "one" else if (value == 2) "two" else "many"
        """.trimIndent()

        assertEquals(1, NoElseIf().lint(code).size)
    }

    @Test
    fun `accepts a when expression`() {
        val code = """
            fun label(value: Int) = when (value) {
                1 -> "one"
                2 -> "two"
                else -> "many"
            }
        """.trimIndent()

        assertEquals(0, NoElseIf().lint(code).size)
    }

    @Test
    fun `accepts a plain if without else-if`() {
        val code = """
            fun label(value: Int) = if (value == 1) "one" else "many"
        """.trimIndent()

        assertEquals(0, NoElseIf().lint(code).size)
    }
}

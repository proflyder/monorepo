package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class LongLambdaTest {
    @Test
    fun `accepts a single multi-line expression`() {
        val code = """
            fun describe(value: Int): String = listOf(value).map {
                when (value) {
                    1 -> "one"
                    2 -> "two"
                    else -> "many"
                }
            }.first()
        """.trimIndent()

        assertEquals(0, LongLambda().lint(code).size)
    }

    @Test
    fun `reports a lambda with more than three body lines`() {
        val code = """
            fun run() {
                listOf(1, 2).forEach {
                    println(it)
                    println(it + 1)
                    println(it + 2)
                    println(it + 3)
                }
            }
        """.trimIndent()

        assertEquals(1, LongLambda().lint(code).size)
    }

    @Test
    fun `accepts a lambda with three body lines`() {
        val code = """
            fun run() {
                listOf(1, 2).forEach {
                    println(it)
                    println(it + 1)
                    println(it + 2)
                }
            }
        """.trimIndent()

        assertEquals(0, LongLambda().lint(code).size)
    }

    @Test
    fun `accepts a test body even with many statements`() {
        val code = """
            fun test() = runTest {
                val first = 1
                val second = 2
                println(first)
                println(second)
            }
        """.trimIndent()

        assertEquals(0, LongLambda().lint(code).size)
    }

    @Test
    fun `accepts scope builders even with long bodies`() {
        val code = """
            fun configure(): StringBuilder = StringBuilder().apply {
                append("a")
                append("b")
                append("c")
                append("d")
            }
        """.trimIndent()

        assertEquals(0, LongLambda().lint(code).size)
    }
}

package dev.proflyder.home.detekt

import io.github.detekt.test.utils.compileContentForTest
import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoInlineJsonInTestsTest {
    @Test
    fun `reports a JSON literal in a test source`() {
        val file = compileContentForTest(
            "val body = \"{\\\"id\\\":\\\"alarm\\\",\\\"hour\\\":7}\"",
            "src/test/kotlin/FixtureTest.kt",
        )

        assertEquals(1, NoInlineJsonInTests().lint(file).size)
    }

    @Test
    fun `reports a raw JSON literal in a test source`() {
        val file = compileContentForTest(
            "val body = \"\"\"{\"id\":\"alarm\"}\"\"\"",
            "src/test/kotlin/FixtureTest.kt",
        )

        assertEquals(1, NoInlineJsonInTests().lint(file).size)
    }

    @Test
    fun `accepts a JSON literal in production code`() {
        val file = compileContentForTest(
            "val body = \"{\\\"id\\\":\\\"alarm\\\"}\"",
            "src/main/kotlin/Fixture.kt",
        )

        assertEquals(0, NoInlineJsonInTests().lint(file).size)
    }

    @Test
    fun `accepts a plain string in a test source`() {
        val file = compileContentForTest(
            "val label = \"alarm\"",
            "src/test/kotlin/FixtureTest.kt",
        )

        assertEquals(0, NoInlineJsonInTests().lint(file).size)
    }
}

package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoCodeLayoutManagerTest {
    @Test
    fun `reports layout managers created for simple lists`() {
        val code = """
            fun bind(list: RecyclerView, context: Context) {
                list.layoutManager = LinearLayoutManager(context)
                list.setLayoutManager(GridLayoutManager(context, 2))
            }
        """.trimIndent()

        assertEquals(4, NoCodeLayoutManager().lint(code).size)
    }

    @Test
    fun `accepts a grid that installs a span size lookup`() {
        val code = """
            fun bind(list: RecyclerView, context: Context, spans: GridLayoutManager.SpanSizeLookup) {
                list.layoutManager = GridLayoutManager(context, 4).apply { spanSizeLookup = spans }
            }
        """.trimIndent()

        assertEquals(0, NoCodeLayoutManager().lint(code).size)
    }
}

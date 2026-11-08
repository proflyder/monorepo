package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoHardcodedUiTextTest {
    @Test
    fun `reports a literal passed to setText`() {
        val code = """
            fun bind(view: android.widget.TextView) {
                view.setText("Alarm")
            }
        """.trimIndent()

        assertEquals(1, NoHardcodedUiText().lint(code).size)
    }

    @Test
    fun `reports a literal Toast message`() {
        val code = """
            fun show(context: android.content.Context) {
                Toast.makeText(context, "Saved", Toast.LENGTH_SHORT).show()
            }
        """.trimIndent()

        assertEquals(1, NoHardcodedUiText().lint(code).size)
    }

    @Test
    fun `reports a literal text property`() {
        val code = """
            fun bind(view: android.widget.TextView) {
                view.text = "Alarm"
            }
        """.trimIndent()

        assertEquals(1, NoHardcodedUiText().lint(code).size)
    }

    @Test
    fun `accepts a resource and an empty string`() {
        val code = """
            fun bind(view: android.widget.TextView, context: android.content.Context) {
                view.setText(R.string.alarm_title)
                view.setText("")
                view.text = context.getString(R.string.alarm_title)
            }
        """.trimIndent()

        assertEquals(0, NoHardcodedUiText().lint(code).size)
    }

    @Test
    fun `accepts a template with an interpolation`() {
        val code = """
            fun bind(view: android.widget.TextView, count: Int) {
                view.text = "${'$'}{count} alarms"
            }
        """.trimIndent()

        assertEquals(0, NoHardcodedUiText().lint(code).size)
    }
}

package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoUiOwnedControlStateTest {
    @Test
    fun `reports a control flipping its own state`() {
        val code = """
            fun flip(view: android.widget.CheckBox) {
                view.isChecked = !view.isChecked
            }
        """.trimIndent()

        assertEquals(1, NoUiOwnedControlState().lint(code).size)
    }

    @Test
    fun `reports a state write inside a listener and a toggle call`() {
        val code = """
            fun bind(view: android.widget.Switch, other: android.widget.Switch) {
                view.setOnCheckedChangeListener { _, checked -> other.isChecked = checked }
                view.setOnClickListener { view.toggle() }
            }
        """.trimIndent()

        assertEquals(2, NoUiOwnedControlState().lint(code).size)
    }

    @Test
    fun `accepts rendering state and sending an intent`() {
        val code = """
            fun bind(view: android.widget.Switch, item: Item, onToggle: (String, Boolean) -> Unit) {
                view.setOnCheckedChangeListener(null)
                view.isChecked = item.enabled
                view.setOnCheckedChangeListener { _, checked -> onToggle(item.id, checked) }
                view.setOnClickListener { viewModel.toggle() }
                view.setOnLongClickListener { toggle() }
            }
        """.trimIndent()

        assertEquals(0, NoUiOwnedControlState().lint(code).size)
    }
}

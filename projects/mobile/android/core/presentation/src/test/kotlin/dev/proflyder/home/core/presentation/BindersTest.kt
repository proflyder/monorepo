package dev.proflyder.home.core.presentation

import android.widget.EditText
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ToggleButton
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BindersTest {
    private val context get() = RuntimeEnvironment.getApplication()

    @Test
    fun aCheckableKeepsTheRenderedValueAfterAClick() {
        val view = ToggleButton(context)
        val intents = mutableListOf<Boolean>()
        val binder = CheckableBinder(view) { intents += it }
        binder.render(false)
        view.performClick()
        assertEquals(listOf(true), intents)
        assertFalse(view.isChecked)
        binder.render(true)
        assertTrue(view.isChecked)
    }

    @Test
    fun aRadioGroupSendsTheChoiceAndShowsOnlyTheRenderedOne() {
        val group = RadioGroup(context)
        val buttons = List(3) { index -> RadioButton(context).apply { id = index + 1 }.also(group::addView) }
        val intents = mutableListOf<Int>()
        val binder = RadioGroupBinder(group, { it }, { it }) { intents += it }
        binder.render(1)
        buttons[2].performClick()
        assertEquals(listOf(3), intents)
        assertEquals(1, group.checkedRadioButtonId)
        binder.render(3)
        assertEquals(3, group.checkedRadioButtonId)
        assertEquals(listOf(3), intents)
    }

    @Test
    fun typingSendsTextAndRenderingDoesNot() {
        val view = EditText(context)
        val intents = mutableListOf<String>()
        val binder = TextBinder(view) { intents += it }
        binder.render("milk")
        view.setText("bread")
        binder.render("bread")
        assertEquals(listOf("bread"), intents)
        assertEquals("bread", view.text.toString())
    }
}

package dev.proflyder.home.core.presentation

import android.widget.CheckBox
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
class SwitchBinderTest {
    @Test
    fun aClickSendsTheIntentButTheSwitchShowsOnlyRenderedState() {
        val view = CheckBox(RuntimeEnvironment.getApplication())
        val intents = mutableListOf<Boolean>()
        val binder = SwitchBinder(view) { intents += it }
        binder.render(false)
        view.performClick()
        assertEquals(listOf(true), intents)
        assertFalse(view.isChecked)
        binder.render(true)
        assertTrue(view.isChecked)
        view.performClick()
        assertEquals(listOf(true, false), intents)
        assertTrue(view.isChecked)
    }
}

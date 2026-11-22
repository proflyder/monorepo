package dev.proflyder.home.core.presentation

import android.view.ContextThemeWrapper
import dev.proflyder.home.core.designsystem.PillSegments
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PillSegmentsBinderTest {
    @Test
    fun aTapSendsTheIntentButThePillsShowOnlyRenderedState() {
        val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), com.google.android.material.R.style.Theme_Material3_DayNight)
        val pills = PillSegments(context).apply { setLabels(listOf("a", "b", "c")) }
        val intents = mutableListOf<String>()
        val binder = PillSegmentsBinder(pills, listOf("a", "b", "c")) { intents += it }
        binder.render("a")

        pills.getChildAt(2).performClick()
        assertEquals(listOf("c"), intents)
        assertEquals(0, pills.selectedIndex)
        assertEquals(listOf(true, false, false), selection(pills))

        binder.render("c")
        assertEquals(2, pills.selectedIndex)
        assertEquals(listOf(false, false, true), selection(pills))

        pills.getChildAt(2).performClick()
        assertEquals(listOf("c"), intents)
    }

    private fun selection(pills: PillSegments): List<Boolean> = (0 until pills.childCount).map { pills.getChildAt(it).isSelected }
}

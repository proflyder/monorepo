package dev.proflyder.home.core.designsystem

import android.view.LayoutInflater
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import dev.proflyder.home.core.theme.AppTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
class FabStylesTest {
    @Test
    fun everyFabHasTheSameOutlineInEveryTheme() {
        val widths = AppTheme.entries.flatMap(::strokes).map { it.first }.distinct()
        assertEquals("one outline width for every FAB and theme: $widths", 1, widths.size)
        assertTrue(widths.single() > 0)
    }

    @Test
    fun everyFabOutlineUsesTheThemeColor() = AppTheme.entries.forEach { theme ->
        val expected = color(themed(theme), R.attr.homeFabStrokeColor)
        strokes(theme).forEach { (_, stroke) -> assertEquals("$theme", expected, stroke) }
    }

    private fun strokes(theme: AppTheme): List<Pair<Int, Int>> {
        val root = LayoutInflater.from(themed(theme)).inflate(R.layout.ds_catalog, null, false)
        val outlined = FAB_IDS.map { root.findViewById<OutlinedFloatingActionButton>(it) }.map { it.strokeWidth to checkNotNull(it.strokeColor).defaultColor }
        val extended = root.findViewById<ExtendedFloatingActionButton>(R.id.ds_catalog_fab_extended)
        return outlined + (extended.strokeWidth to checkNotNull(extended.strokeColor).defaultColor)
    }

    private companion object {
        val FAB_IDS = listOf(R.id.ds_catalog_fab, R.id.ds_catalog_fab_small, R.id.ds_catalog_fab_quick)
    }
}

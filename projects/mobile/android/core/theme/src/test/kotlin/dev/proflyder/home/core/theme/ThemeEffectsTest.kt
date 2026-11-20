package dev.proflyder.home.core.theme

import android.content.Context
import android.graphics.Color
import android.util.TypedValue
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "night")
class ThemeEffectsTest {
    @Test
    fun everyThemeOutlinesFabsWithTheSameWidth() {
        val widths = AppTheme.entries.map { dimension(themed(it), R.attr.homeFabStrokeWidth) }
        assertTrue("stroke widths $widths", widths.all { it > 0f })
        assertEquals(1, widths.toSet().size)
    }

    @Test
    fun everyThemeGlowsInItsOwnAccent() {
        AppTheme.entries.forEach(::assertGlow)
    }

    private fun assertGlow(theme: AppTheme) {
        val context = themed(theme)
        val glow = MaterialColors.getColor(context, R.attr.homeFabGlowColor, Color.BLACK)
        assertNotEquals("$theme glow", Color.BLACK, glow)
        assertEquals("$theme glow", MaterialColors.getColor(context, R.attr.homeFabStrokeColor, 0), glow)
        assertTrue("$theme elevation", dimension(context, R.attr.homeFabElevation) > 0f)
    }

    private fun dimension(context: Context, attr: Int): Float {
        val value = TypedValue()
        context.theme.resolveAttribute(attr, value, true)
        return value.getDimension(context.resources.displayMetrics)
    }

    private fun themed(theme: AppTheme): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            MaterialR.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, theme.overlayRes)
    }
}

package dev.proflyder.home.core.theme

import android.content.Context
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

/**
 * The XML overlays are what the View system actually uses; the palette data is
 * what the registry, previews and contrast checks use. This proves both say
 * the same thing for every role, in light and in night resources.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ThemeOverlayConsistencyTest {
    @Test
    @Config(qualifiers = "notnight")
    fun lightOverlaysResolveToThePaletteData() {
        val problems = AppTheme.entries.flatMap { mismatches(it, it.palette.light) }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    @Test
    @Config(qualifiers = "night")
    fun nightOverlaysResolveToThePaletteData() {
        val problems = AppTheme.entries.flatMap { mismatches(it, it.palette.dark) }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    private fun mismatches(theme: AppTheme, scheme: ColorScheme): List<String> {
        val context = themed(theme)
        return ColorRole.entries.mapNotNull { role ->
            val resolved = MaterialColors.getColor(context, role.attr, 0)
            "$theme/$role: overlay #%08X, palette #%08X".format(resolved, scheme[role])
                .takeIf { resolved != scheme[role] }
        }
    }

    /** Static overlay only: Robolectric has no wallpaper colours. */
    private fun themed(theme: AppTheme): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            MaterialR.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, theme.overlayRes)
    }
}

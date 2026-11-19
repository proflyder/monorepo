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

    private fun themed(theme: AppTheme): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            MaterialR.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, theme.overlayRes)
    }
}

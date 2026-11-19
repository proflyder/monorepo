package dev.proflyder.home.core.theme

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ThemeRegistryTest {
    @Test
    fun everyThemeDefinesEveryRoleAsAnOpaqueColourInBothBrightnesses() {
        val problems = AppTheme.entries.flatMap { theme ->
            schemes(theme).flatMap { (name, scheme) -> transparentRoles(theme, name, scheme) }
        }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    @Test
    fun keysAreUniqueAndUnknownKeysFallBackToTheDefault() {
        assertEquals(AppTheme.entries.size, AppTheme.entries.map { it.key }.toSet().size)
        assertEquals(AppTheme.CURRENT, AppTheme.fromKey("missing"))
        assertEquals(AppTheme.NEON, AppTheme.fromKey("neon"))
        assertEquals(ThemeMode.SYSTEM, ThemeMode.fromKey(null))
        assertEquals(ThemeMode.DARK, ThemeMode.fromKey("dark"))
    }

    @Test
    fun theCurrentThemeKeepsTheOriginalPalette() {
        assertEquals(rgb(0x0E7C66), AppTheme.CURRENT.palette.light.primary)
        assertEquals(rgb(0x6FDBC0), AppTheme.CURRENT.palette.dark.primary)
        assertEquals(rgb(0x0E1512), AppTheme.CURRENT.palette.dark.surface)
        assertEquals(AppTheme.CURRENT.palette, AppTheme.SYSTEM.palette)
    }

    @Test
    fun textPairsMeetWcagAa() {
        val problems = AppTheme.entries.flatMap { theme ->
            schemes(theme).flatMap { (name, scheme) -> contrastProblems(theme, name, scheme) }
        }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    @Test
    fun contrastMatchesTheWcagReferenceValues() {
        assertEquals(21.0, Contrast.ratio(rgb(0x000000), rgb(0xFFFFFF)), DELTA)
        assertEquals(1.0, Contrast.ratio(rgb(0x777777), rgb(0x777777)), DELTA)
        assertEquals(4.48, Contrast.ratio(rgb(0x777777), rgb(0xFFFFFF)), 0.01)
    }

    private fun schemes(theme: AppTheme) = listOf("light" to theme.palette.light, "dark" to theme.palette.dark)

    private fun transparentRoles(theme: AppTheme, name: String, scheme: ColorScheme): List<String> =
        ColorRole.entries.filter { scheme[it] ushr ALPHA_SHIFT != OPAQUE_ALPHA }
            .map { "$theme/$name/$it is not opaque" }

    private fun contrastProblems(theme: AppTheme, name: String, scheme: ColorScheme): List<String> =
        TEXT_PAIRS.mapNotNull { (text, background) ->
            val ratio = Contrast.ratio(scheme[text], scheme[background])
            "$theme/$name $text on $background = %.2f".format(ratio).takeIf { ratio < Contrast.AA_TEXT }
        }

    private companion object {
        const val DELTA = 0.001
        const val ALPHA_SHIFT = 24
        const val OPAQUE_ALPHA = 0xFF

        val SURFACES = listOf(
            ColorRole.SURFACE,
            ColorRole.SURFACE_CONTAINER_LOW,
            ColorRole.SURFACE_CONTAINER,
            ColorRole.SURFACE_CONTAINER_HIGH,
            ColorRole.SURFACE_CONTAINER_HIGHEST,
        )

        val TEXT_PAIRS: List<Pair<ColorRole, ColorRole>> = listOf(
            ColorRole.ON_PRIMARY to ColorRole.PRIMARY,
            ColorRole.ON_PRIMARY_CONTAINER to ColorRole.PRIMARY_CONTAINER,
            ColorRole.ON_SECONDARY to ColorRole.SECONDARY,
            ColorRole.ON_SECONDARY_CONTAINER to ColorRole.SECONDARY_CONTAINER,
            ColorRole.ON_TERTIARY to ColorRole.TERTIARY,
            ColorRole.ON_TERTIARY_CONTAINER to ColorRole.TERTIARY_CONTAINER,
            ColorRole.ON_ERROR to ColorRole.ERROR,
            ColorRole.ON_ERROR_CONTAINER to ColorRole.ERROR_CONTAINER,
            ColorRole.PRIMARY to ColorRole.SURFACE,
        ) + SURFACES.flatMap { surface ->
            listOf(ColorRole.ON_SURFACE, ColorRole.ON_SURFACE_VARIANT).map { it to surface }
        }
    }
}

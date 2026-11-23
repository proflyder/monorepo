package dev.proflyder.home.core.launchericon

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import dev.proflyder.home.core.theme.AppTheme
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PackageManagerLauncherIconsTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val icons = PackageManagerLauncherIcons(context)

    @Test
    fun freshInstallShowsManifestDefault() {
        assertEquals(LauncherAliases.manifestDefault, icons.shown())
        assertEquals(listOf(LauncherAliases.manifestDefault), enabledThemes())
    }

    @Test
    fun everyThemeLeavesExactlyItsAliasEnabled() {
        AppTheme.entries.forEach { theme ->
            icons.show(theme)
            assertEquals(listOf(theme), enabledThemes())
            assertEquals(theme, icons.shown())
        }
    }

    @Test
    fun switchingBackRestoresDefaultAlias() {
        icons.show(AppTheme.NEON)
        icons.show(AppTheme.DEFAULT)
        assertEquals(listOf(AppTheme.DEFAULT), enabledThemes())
    }

    @Test
    fun repeatedShowDoesNotRewriteComponentState() {
        icons.show(AppTheme.NEON)
        val before = states()
        icons.show(AppTheme.NEON)
        assertEquals(before, states())
    }

    @Test
    fun aliasNamesAreDistinctAndInLauncherPackage() {
        val names = AppTheme.entries.map(LauncherAliases::className)
        assertEquals(names.size, names.toSet().size)
        names.forEach { assertEquals(LauncherAliases.PACKAGE, it.substringBeforeLast('.')) }
    }

    private fun states(): List<Int> = AppTheme.entries.map { context.packageManager.getComponentEnabledSetting(component(it)) }

    private fun enabledThemes(): List<AppTheme> = AppTheme.entries.filter(::isEnabled)

    private fun isEnabled(theme: AppTheme): Boolean = when (context.packageManager.getComponentEnabledSetting(component(theme))) {
        PackageManager.COMPONENT_ENABLED_STATE_ENABLED -> true
        PackageManager.COMPONENT_ENABLED_STATE_DEFAULT -> theme == LauncherAliases.manifestDefault
        else -> false
    }

    private fun component(theme: AppTheme) = ComponentName(context.packageName, LauncherAliases.className(theme))
}

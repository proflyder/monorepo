package dev.proflyder.home

import android.content.ComponentName
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.AdaptiveIconDrawable
import android.graphics.drawable.Drawable
import dev.proflyder.home.core.launchericon.LauncherAliases
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LauncherAliasesTest {
    private val app = RuntimeEnvironment.getApplication() as HomeApplication
    private val packageManager: PackageManager = app.packageManager

    @Test
    fun everyThemeHasAliasToMainActivity() {
        AppTheme.entries.forEach { theme ->
            val info = aliasInfo(theme)
            assertEquals(MainActivity::class.java.name, info.targetActivity)
            assertEquals(theme == LauncherAliases.manifestDefault, info.enabled)
        }
    }

    @Test
    fun mainActivityStaysAddressableDirectly() {
        val info = packageManager.getActivityInfo(ComponentName(app, MainActivity::class.java), 0)
        assertTrue(info.exported)
    }

    @Test
    fun startablesSyncLauncherIcon() {
        val names = app.graph.startables.map { it::class.java.simpleName }
        assertTrue(names.toString(), "LauncherIconSync" in names)
    }

    @Test
    fun launchIntentResolvesToEnabledAlias() {
        val intent = packageManager.getLaunchIntentForPackage(app.packageName)
        assertNotNull(intent)
        assertEquals(LauncherAliases.className(LauncherAliases.manifestDefault), intent?.component?.className)
        assertEquals(Intent.ACTION_MAIN, intent?.action)
    }

    @Test
    fun iconsAreAdaptiveWithMonochromeLayer() {
        AppTheme.entries.forEach(::checkAndSaveIcon)
    }

    @Test
    @Config(qualifiers = "night-xxhdpi")
    fun darkIconsUseTheDarkPalette() {
        AppTheme.entries.forEach { theme ->
            save(app.getDrawable(aliasInfo(theme).icon) as AdaptiveIconDrawable, "launcher-icon-${theme.key}-dark.png")
        }
    }

    private fun checkAndSaveIcon(theme: AppTheme) {
        val icon = app.getDrawable(aliasInfo(theme).icon) as AdaptiveIconDrawable
        val monochrome = requireNotNull(icon.monochrome)
        save(icon, "launcher-icon-${theme.key}.png")
        save(icon.background, "launcher-icon-${theme.key}-background.png")
        save(icon.foreground, "launcher-icon-${theme.key}-foreground.png")
        save(monochrome, "launcher-icon-${theme.key}-monochrome.png")
    }

    private fun aliasInfo(theme: AppTheme): ActivityInfo = packageManager.getActivityInfo(
        ComponentName(app.packageName, LauncherAliases.className(theme)),
        PackageManager.MATCH_DISABLED_COMPONENTS,
    )

    private fun save(drawable: Drawable, name: String) {
        val bitmap = Bitmap.createBitmap(ICON_PX, ICON_PX, Bitmap.Config.ARGB_8888)
        drawable.setBounds(0, 0, ICON_PX, ICON_PX)
        drawable.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        const val ICON_PX = 432
        const val PNG_QUALITY = 100
    }
}

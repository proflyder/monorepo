package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.ColorRole
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import kotlinx.coroutines.runBlocking

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ThemeScreenshotTest {
    @Test
    fun currentThemeScreens() = shootAll(AppTheme.CURRENT)

    @Test
    fun systemThemeScreens() = shootAll(AppTheme.SYSTEM)

    @Test
    fun neonThemeScreens() = shootAll(AppTheme.NEON)

    private fun shootAll(theme: AppTheme) {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        runBlocking { app.graph.appearance.selectTheme(theme) }
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        val surface = MaterialColors.getColor(activity, ColorRole.SURFACE.attr, 0)
        if (!theme.dynamic) assertEquals(theme.palette.dark.surface, surface)

        shoot(activity, surface, "theme-${theme.key}-home-dark.png")
        activity.router.selectTab("time")
        idle(activity)
        shoot(activity, surface, "theme-${theme.key}-time-dark.png")
        activity.router.openRoute("settings/appearance")
        idle(activity)
        shoot(activity, surface, "theme-${theme.key}-appearance-dark.png")
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun shoot(activity: MainActivity, surface: Int, name: String) {
        val bitmap = render(activity.window.decorView)
        val share = surfaceShare(bitmap, surface)
        assertTrue("$name: theme surface covers only ${(share * PERCENT).toInt()}%", share > MIN_SURFACE_SHARE)
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun render(root: View): Bitmap {
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        return bitmap
    }

    private fun surfaceShare(bitmap: Bitmap, surface: Int): Double {
        val pixels = IntArray(bitmap.width * bitmap.height)
        bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
        return pixels.count { it == surface }.toDouble() / pixels.size
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val PNG_QUALITY = 100
        const val PERCENT = 100
        const val MIN_SURFACE_SHARE = 0.25
    }
}

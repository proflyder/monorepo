package dev.proflyder.home.core.designsystem

import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CatalogScreenshotTest {
    @Test
    fun rendersInEveryThemeDark() = AppTheme.entries.forEach { shoot(it, "dark") }

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun rendersInEveryThemeLight() = AppTheme.entries.forEach { shoot(it, "light") }

    private fun shoot(theme: AppTheme, mode: String) {
        val context = themed(theme)
        val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
        assertTrue(night != Configuration.UI_MODE_NIGHT_UNDEFINED)
        val root = DesignCatalog.inflate(LayoutInflater.from(context), null)
        layout(root)
        layout(root)
        DesignCatalog.sections.forEach { (name, id) ->
            val section = root.findViewById<View>(id)
            assertTrue("$name is empty", section.width > 0 && section.height > 0)
            save(render(section), "catalog-${theme.key}-$mode-$name.png")
        }
    }

    private fun layout(root: View) {
        val metrics = RuntimeEnvironment.getApplication().resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        root.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        root.layout(0, 0, root.measuredWidth, root.measuredHeight)
    }

    private fun render(view: View): Bitmap {
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        return bitmap
    }

    private fun save(bitmap: Bitmap, name: String) {
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        const val WIDTH_DP = 411
        const val PNG_QUALITY = 100
    }
}

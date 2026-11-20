package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import com.google.android.material.appbar.MaterialToolbar
import java.io.File
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ToolbarMenuScreenshotTest {
    @Test
    fun theActionsMenuIsARoundedTonalCardWithTwoIconRowsInTheDarkTheme() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        toolbar.menu.performIdentifierAction(R.id.action_menu, 0)
        shadowOf(activity.mainLooper).idle()
        val menu = requireNotNull(activity.actionsMenu.contentView) {
            "the menu must be open for the screenshot"
        }

        val root = activity.window.decorView
        val metrics = root.resources.displayMetrics
        val width = (411 * metrics.density).toInt()
        val height = (891 * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)

        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        root.draw(canvas)
        drawMenu(canvas, menu, toolbar)
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, "toolbar-actions-menu-dark.png").outputStream().use {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    private fun drawMenu(canvas: Canvas, menu: View, toolbar: MaterialToolbar) {
        menu.measure(unspecified(), unspecified())
        menu.layout(0, 0, menu.measuredWidth, menu.measuredHeight)
        val anchor = toolbar.findViewById<View>(R.id.action_menu)
        val location = IntArray(2)
        anchor.getLocationInWindow(location)
        val left = location[0] + anchor.width - menu.measuredWidth
        canvas.save()
        canvas.translate(left.toFloat(), (location[1] + anchor.height).toFloat())
        menu.draw(canvas)
        canvas.restore()
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun unspecified(): Int = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)
}

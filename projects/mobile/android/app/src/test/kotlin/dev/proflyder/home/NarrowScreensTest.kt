package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.widget.TextView
import androidx.core.widget.NestedScrollView
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.designsystem.ButtonRow
import dev.proflyder.home.core.designsystem.ToolbarTitle
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.connection.ConnectionDestination
import dev.proflyder.home.feature.push.PushSettingsDestination
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.feature.connection.impl.R as ConnectionR

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NarrowScreensTest {
    @Test
    @Config(sdk = [35], qualifiers = "ru-w360dp-h780dp-night-xxhdpi")
    fun connectionButtonsStackOnNarrowScreens() {
        val activity = open(ConnectionDestination, 360)
        activity.findViewById<View>(ConnectionR.id.connection_clear_token).visibility = View.VISIBLE
        layout(activity.window.decorView, 360)
        val row = activity.findViewById<ButtonRow>(ConnectionR.id.connection_actions)
        assertTrue("two Russian actions do not fit side by side at 360dp", row.isStacked)
        assertButtons(row)
        val main = activity.findViewById<MaterialButton>(ConnectionR.id.connection_test)
        val secondary = activity.findViewById<MaterialButton>(ConnectionR.id.connection_clear_token)
        assertTrue("main action is on top", main.top < secondary.top)
        assertEquals("stacked buttons take the full width", row.width - row.paddingLeft - row.paddingRight, main.width)
        scrollTo(row)
        shoot(activity, "narrow-360-connection.png")
    }

    @Test
    @Config(sdk = [35], qualifiers = "ru-w360dp-h780dp-night-xxhdpi")
    fun toolbarTitleFitsAt360() = assertTitle(360)

    @Test
    @Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun toolbarTitleFitsAt411() = assertTitle(411)

    private fun assertTitle(widthDp: Int) {
        val activity = open(PushSettingsDestination, widthDp)
        val title = activity.findViewById<ToolbarTitle>(R.id.toolbar_title)
        val layout = checkNotNull(title.layout)
        assertTrue("at most two lines", layout.lineCount <= 2)
        (0 until layout.lineCount).forEach { assertEquals("no ellipsis", 0, layout.getEllipsisCount(it)) }
        assertLinesEndOnWords(title)
        val right = IntArray(2).also(title::getLocationInWindow)[0] + title.width
        assertTrue("title ends inside the screen", right <= activity.window.decorView.width)
        assertTrue("text fits the view", (0 until layout.lineCount).all { layout.getLineWidth(it) <= title.width - title.totalPaddingLeft - title.totalPaddingRight + 1 })
        shoot(activity, "narrow-$widthDp-push-title.png")
    }

    private fun assertLinesEndOnWords(title: TextView) {
        val layout = title.layout
        val text = title.text
        (0 until layout.lineCount - 1).forEach { line ->
            val end = layout.getLineEnd(line)
            assertTrue("line $line breaks inside a word", text[end - 1] == ' ')
        }
    }

    private fun scrollTo(view: View) {
        val scroll = generateSequence(view.parent as? View) { it.parent as? View }.filterIsInstance<NestedScrollView>().first()
        val target = IntArray(2).also(view::getLocationInWindow)[1] - IntArray(2).also(scroll::getLocationInWindow)[1]
        scroll.scrollBy(0, target - scroll.height / 2)
    }

    private fun assertButtons(row: ButtonRow) = (0 until row.childCount).map(row::getChildAt).forEach { child ->
        val button = child as MaterialButton
        assertEquals("${button.text} wraps", 1, button.lineCount)
        assertFalse("${button.text} is cut", button.layout.getEllipsisCount(0) > 0)
    }

    private fun open(destination: Destination, widthDp: Int): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        app.graph.appearance.selectTheme(AppTheme.CURRENT)
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        activity.router.navigate(destination)
        idle(activity)
        layout(activity.window.decorView, widthDp)
        return activity
    }

    private fun layout(root: View, widthDp: Int) {
        val density = root.resources.displayMetrics.density
        val width = (widthDp * density).toInt()
        val height = (root.resources.configuration.screenHeightDp * density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun shoot(activity: MainActivity, name: String) {
        val root = activity.window.decorView
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val PNG_QUALITY = 100
    }
}

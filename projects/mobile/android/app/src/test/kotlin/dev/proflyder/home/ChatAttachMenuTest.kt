package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.view.WindowManager
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.feature.chat.impl.R as ChatR
import kotlinx.coroutines.runBlocking

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ChatAttachMenuTest {
    @Test
    @Config(sdk = [35], qualifiers = "ru-w360dp-h640dp-night-xxhdpi")
    fun attachMenuOpensAboveTheAnchorAt640() = assertAttachMenu("chat-attach-menu-640.png")

    @Test
    @Config(sdk = [35], qualifiers = "ru-w411dp-h800dp-night-xxhdpi")
    fun attachMenuOpensAboveTheAnchorAt800() = assertAttachMenu("chat-attach-menu-800.png")

    @Test
    @Config(sdk = [35], qualifiers = "ru-w411dp-h800dp-night-xxhdpi")
    fun attachMenuFollowsTheAnchorWhenTheKeyboardHides() {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        val activity = openChat(app)
        val anchor = activity.findViewById<View>(ChatR.id.chat_attach)
        val panel = activity.findViewById<View>(ChatR.id.chat_panel)
        panel.translationY = -KEYBOARD_SHIFT * anchor.resources.displayMetrics.density
        anchor.performClick()
        idle(activity)
        val popup = checkNotNull(shadowOf(app).latestPopupWindow)
        val raised = popup.contentView.rootView.layoutParams as WindowManager.LayoutParams
        val raisedBottom = raised.y + popup.height
        panel.translationY = 0f
        activity.window.decorView.viewTreeObserver.dispatchOnPreDraw()
        idle(activity)
        val params = popup.contentView.rootView.layoutParams as WindowManager.LayoutParams
        val anchorTop = top(anchor)
        assertTrue("menu moved with the anchor", params.y + popup.height > raisedBottom)
        assertTrue("menu is above the anchor", params.y + popup.height <= anchorTop)
        assertTrue("menu touches the anchor", anchorTop - (params.y + popup.height) <= HomePopupMenu.ANCHOR_GAP)
    }

    private fun openChat(app: HomeApplication): MainActivity {
        runBlocking { app.graph.appearance.selectTheme(AppTheme.CURRENT) }
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        activity.router.selectTab("chat")
        idle(activity)
        layout(activity.window.decorView)
        return activity
    }

    private fun assertAttachMenu(name: String) {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        val activity = openChat(app)
        val anchor = activity.findViewById<View>(ChatR.id.chat_attach)
        anchor.performClick()
        idle(activity)
        val popup = checkNotNull(shadowOf(app).latestPopupWindow)
        assertNotNull("attach menu is shown", popup)
        val params = popup.contentView.rootView.layoutParams as WindowManager.LayoutParams
        val anchorTop = top(anchor)
        val navTop = top(activity.findViewById(R.id.home_notice_anchor))
        assertTrue("menu is above the anchor", params.y + popup.height <= anchorTop)
        assertTrue("menu is above the bottom navigation", params.y + popup.height <= navTop)
        assertTrue("menu is inside the screen", params.y >= 0)
        shoot(activity, popup.contentView, params, name)
    }

    private fun top(view: View): Int = IntArray(2).also(view::getLocationInWindow)[1]

    private fun layout(root: View) {
        val density = root.resources.displayMetrics.density
        val width = (root.resources.configuration.screenWidthDp * density).toInt()
        val height = (root.resources.configuration.screenHeightDp * density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun shoot(activity: MainActivity, menu: View, params: WindowManager.LayoutParams, name: String) {
        val root = activity.window.decorView
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        root.draw(canvas)
        menu.measure(exactly(params.width), exactly(params.height))
        menu.layout(0, 0, params.width, params.height)
        canvas.translate(params.x.toFloat(), params.y.toFloat())
        menu.draw(canvas)
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val PNG_QUALITY = 100
        const val KEYBOARD_SHIFT = 300f
    }
}

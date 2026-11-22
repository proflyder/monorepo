package dev.proflyder.home.core.designsystem

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w360dp-h200dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class RevealBinderTest {
    @Test
    fun bannerGrowsAndFadesInThenCollapsesAndFadesOut() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val root = LinearLayout(activity).apply { orientation = LinearLayout.VERTICAL }
        val banner = block(activity, BANNER_COLOR, BANNER_HEIGHT_DP).apply { visibility = View.GONE }
        val content = block(activity, CONTENT_COLOR, CONTENT_HEIGHT_DP)
        root.addView(banner)
        root.addView(content)
        activity.setContentView(root)
        settle()
        val binder = RevealBinder(banner)
        val bannerHeight = dp(activity, BANNER_HEIGHT_DP)
        assertEquals(0, content.top)
        save(root, "reveal-banner-hidden.png")

        binder.render(true)
        seek(binder, HALF)
        assertTrue("banner grows", banner.height in 1 until bannerHeight)
        assertTrue("banner fades in", banner.alpha in 0.01f..0.99f)
        assertEquals(banner.height, content.top)
        save(root, "reveal-banner-entering.png")

        finish(binder)
        assertEquals(bannerHeight, banner.height)
        assertEquals(1f, banner.alpha)
        assertEquals(ViewGroup.LayoutParams.WRAP_CONTENT, banner.layoutParams.height)
        save(root, "reveal-banner-shown.png")

        binder.render(false)
        seek(binder, HALF)
        assertTrue("banner shrinks", banner.height in 1 until bannerHeight)
        assertTrue("banner fades out", banner.alpha in 0.01f..0.99f)
        finish(binder)
        assertEquals(View.GONE, banner.visibility)
        assertEquals(0, content.top)
        assertEquals(1f, banner.alpha)
    }

    private fun block(activity: Activity, color: Int, heightDp: Int): TextView = TextView(activity).apply {
        setBackgroundColor(color)
        layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        minHeight = dp(activity, heightDp)
    }

    private fun dp(activity: Activity, value: Int): Int = (value * activity.resources.displayMetrics.density).toInt()

    private fun seek(binder: RevealBinder, fraction: Float) {
        val animator = checkNotNull(binder.animator)
        animator.pause()
        animator.currentPlayTime = (animator.duration * fraction).toLong()
        settle()
    }

    private fun finish(binder: RevealBinder) {
        checkNotNull(binder.animator).end()
        settle()
    }

    private fun settle() {
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun save(root: View, name: String) {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        val BANNER_COLOR = Color.rgb(0x3A, 0x5B, 0x8C)
        val CONTENT_COLOR = Color.rgb(0x22, 0x22, 0x22)
        const val BANNER_HEIGHT_DP = 48
        const val CONTENT_HEIGHT_DP = 120
        const val HALF = 0.5f
        const val PNG_QUALITY = 100
    }
}

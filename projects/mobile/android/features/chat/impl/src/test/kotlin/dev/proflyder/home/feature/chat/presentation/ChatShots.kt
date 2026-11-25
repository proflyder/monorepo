package dev.proflyder.home.feature.chat.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppThemes
import java.io.File
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import androidx.appcompat.view.ContextThemeWrapper as ThemeWrapper
import com.google.android.material.R as MaterialR

internal object ChatShots {
    const val WIDTH_DP = 411
    const val HEIGHT_DP = 891
    const val STATUS_BAR_DP = 32
    const val NAV_BAR_DP = 24
    const val KEYBOARD_DP = 300
    private const val PNG_QUALITY = 100
    private const val SETTLE_PASSES = 3

    fun themed(theme: AppTheme): Context {
        val base = ThemeWrapper(RuntimeEnvironment.getApplication(), MaterialR.style.Theme_Material3_DayNight_NoActionBar)
        return AppThemes.wrap(base, theme)
    }

    fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).toInt()

    fun layout(view: View) {
        val width = dp(view, WIDTH_DP)
        val height = dp(view, HEIGHT_DP)
        view.measure(exactly(width), exactly(height))
        view.layout(0, 0, width, height)
    }

    fun applyInsets(root: View, keyboard: Int, statusBar: Int = dp(root, STATUS_BAR_DP)) {
        val insets = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, statusBar, 0, 0))
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, dp(root, NAV_BAR_DP)))
            .setInsets(WindowInsetsCompat.Type.ime(), Insets.of(0, 0, 0, keyboard))
            .setVisible(WindowInsetsCompat.Type.ime(), keyboard > 0)
            .build()
        repeat(SETTLE_PASSES) {
            layout(root)
            ViewCompat.dispatchApplyWindowInsets(root, insets)
            shadowOf(android.os.Looper.getMainLooper()).idle()
        }
        layout(root)
    }

    fun save(name: String, view: View, keyboard: Int = 0, keyboardColor: Int = 0) {
        layout(view)
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        view.draw(canvas)
        if (keyboard > 0) {
            canvas.clipRect(0, view.height - keyboard, view.width, view.height)
            canvas.drawColor(keyboardColor)
        }
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)
}

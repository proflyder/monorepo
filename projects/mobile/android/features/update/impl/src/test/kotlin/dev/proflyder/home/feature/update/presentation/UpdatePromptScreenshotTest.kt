package dev.proflyder.home.feature.update.presentation

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import com.google.android.material.color.MaterialColors
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppThemes
import dev.proflyder.home.feature.update.AvailableUpdate
import dev.proflyder.home.feature.update.UpdateCenter
import dev.proflyder.home.feature.update.UpdateFailureKind
import dev.proflyder.home.feature.update.UpdatePhase
import dev.proflyder.home.feature.update.UpdateState
import dev.proflyder.home.feature.update.impl.R
import dev.proflyder.home.feature.update.impl.databinding.DialogUpdatePromptBinding
import java.io.File
import kotlin.math.max
import kotlin.math.min
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLooper
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class UpdatePromptScreenshotTest {
    private class FakeCenter(state: UpdateState) : UpdateCenter {
        private val mutable = MutableStateFlow(state)
        override val supported: Boolean = true
        override val state: StateFlow<UpdateState> = mutable

        override fun checkNow() = Unit

        override fun signIn() = Unit

        override fun install() = Unit

        override fun dismissPrompt() = Unit
    }

    class DialogHostActivity : Activity() {
        override fun onCreate(savedInstanceState: Bundle?) {
            setTheme(MaterialR.style.Theme_Material3_DayNight_NoActionBar)
            super.onCreate(savedInstanceState)
        }
    }

    @Test
    fun emeraldDarkAvailableShowsVersionNotesAndTheUpdateAction() =
        shootAvailable("update-prompt-available-emerald-dark.png")

    @Test
    @Config(qualifiers = LIGHT)
    fun emeraldLightAvailableShowsVersionNotesAndTheUpdateAction() =
        shootAvailable("update-prompt-available-emerald-light.png")

    @Test
    fun emeraldDarkDownloadingShowsDeterminateProgressWithoutActions() =
        shootDownloading("update-prompt-downloading-emerald-dark.png")

    @Test
    @Config(qualifiers = LIGHT)
    fun emeraldLightDownloadingShowsDeterminateProgressWithoutActions() =
        shootDownloading("update-prompt-downloading-emerald-light.png")

    @Test
    fun emeraldDarkSignInExplainsTheTesterAccount() =
        shootSignIn("update-prompt-sign-in-emerald-dark.png")

    @Test
    @Config(qualifiers = LIGHT)
    fun emeraldLightSignInExplainsTheTesterAccount() =
        shootSignIn("update-prompt-sign-in-emerald-light.png")

    private fun shootAvailable(name: String) {
        val state = UpdateState(
            currentVersion = "0.1.0-old",
            phase = UpdatePhase.AVAILABLE,
            available = AvailableUpdate("0.2.0-abc1234", 2L, "Будильники: тихий холодный старт\nЧат: меню вложений"),
            promptVisible = true,
        )
        val rendered = shoot(state, name)

        assertEquals(View.VISIBLE, rendered.binding.updateNotes.visibility)
        assertEquals("Обновить", rendered.binding.updateAction.text.toString())
        assertEquals(View.VISIBLE, rendered.binding.updateLater.visibility)
        rendered.assertReadable()
    }

    private fun shootDownloading(name: String) {
        val state = UpdateState(
            currentVersion = "0.1.0-old",
            phase = UpdatePhase.DOWNLOADING,
            available = AvailableUpdate("0.2.0-abc1234", 2L, "Будильники: тихий холодный старт"),
            progressPercent = 42,
            promptVisible = true,
        )
        val rendered = shoot(state, name)

        assertEquals(View.VISIBLE, rendered.binding.updateProgress.visibility)
        assertFalse(rendered.binding.updateProgress.isIndeterminate)
        assertEquals(42, rendered.binding.updateProgress.progress)
        assertEquals(View.GONE, rendered.binding.updateAction.visibility)
        assertEquals(View.GONE, rendered.binding.updateLater.visibility)
        rendered.assertReadable()
    }

    private fun shootSignIn(name: String) {
        val state = UpdateState(
            currentVersion = "0.1.0-old",
            phase = UpdatePhase.SIGN_IN_REQUIRED,
            failure = UpdateFailureKind.SIGN_IN_REQUIRED,
            promptVisible = true,
        )
        val rendered = shoot(state, name)

        assertEquals(View.VISIBLE, rendered.binding.updateMessage.visibility)
        assertEquals(
            rendered.binding.updateMessage.context.getString(R.string.home_update_sign_in_explanation),
            rendered.binding.updateMessage.text.toString(),
        )
        assertEquals("Войти", rendered.binding.updateAction.text.toString())
        rendered.assertReadable()
    }

    private fun shoot(state: UpdateState, name: String): Rendered {
        val activity = Robolectric.buildActivity(DialogHostActivity::class.java).setup().get()
        val themed = AppThemes.wrap(activity, AppTheme.EMERALD)
        val binding = DialogUpdatePromptBinding.inflate(LayoutInflater.from(themed))
        UpdatePromptContent(themed, binding, FakeCenter(state)).render(state)
        val dialog = MaterialAlertDialogBuilder(themed).setView(binding.root).create()
        dialog.show()
        ShadowLooper.idleMainLooper()
        val decor = dialog.window!!.decorView
        assertNotNull("the dialog window must paint the design-system surface", decor.background)
        val width = dp(themed, CARD_WIDTH_DP)
        val maxHeight = dp(themed, CARD_MAX_HEIGHT_DP)
        decor.measure(exactly(width), View.MeasureSpec.makeMeasureSpec(maxHeight, View.MeasureSpec.AT_MOST))
        decor.layout(0, 0, decor.measuredWidth, decor.measuredHeight)
        val surface = MaterialColors.getColor(themed, MaterialR.attr.colorSurface, Color.BLACK)
        val padding = dp(themed, SCREEN_PADDING_DP)
        savePng(decor, surface, padding, name)
        dialog.dismiss()
        return Rendered(binding, surface)
    }

    private class Rendered(val binding: DialogUpdatePromptBinding, private val surface: Int) {
        fun assertReadable() {
            assertContrast(binding.updateTitle, surface)
            assertContrast(binding.updateVersion, surface)
            assertContrast(binding.updateStatus, surface)
            assertContrast(binding.updateNotes, surface)
        }

        private fun assertContrast(view: View, background: Int) {
            val text = (view as android.widget.TextView).currentTextColor
            assertTrue("contrast ${ratio(text, background)}", ratio(text, background) >= READABLE_RATIO)
        }
    }

    private fun savePng(decor: View, surface: Int, padding: Int, name: String) {
        val width = decor.measuredWidth + padding * 2
        val height = decor.measuredHeight + padding * 2
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.drawColor(surface)
        canvas.save()
        canvas.translate(padding.toFloat(), padding.toFloat())
        decor.draw(canvas)
        canvas.restore()
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun dp(context: android.content.Context, value: Int): Int =
        (value * context.resources.displayMetrics.density).toInt()

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val LIGHT = "ru-rRU-w411dp-h891dp-notnight-xxhdpi"
        const val CARD_WIDTH_DP = 360
        const val CARD_MAX_HEIGHT_DP = 640
        const val SCREEN_PADDING_DP = 24
        const val READABLE_RATIO = 4.5

        fun ratio(foreground: Int, background: Int): Double {
            val lighter = max(luminance(foreground), luminance(background))
            val darker = min(luminance(foreground), luminance(background))
            return (lighter + 0.05) / (darker + 0.05)
        }

        fun luminance(color: Int): Double {
            fun channel(value: Int): Double {
                val normalized = value / 255.0
                return if (normalized <= 0.03928) normalized / 12.92 else Math.pow((normalized + 0.055) / 1.055, 2.4)
            }
            return 0.2126 * channel(Color.red(color)) + 0.7152 * channel(Color.green(color)) + 0.0722 * channel(Color.blue(color))
        }
    }
}

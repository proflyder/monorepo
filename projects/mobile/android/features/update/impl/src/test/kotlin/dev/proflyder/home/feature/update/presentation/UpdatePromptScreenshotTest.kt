package dev.proflyder.home.feature.update.presentation

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.feature.update.AvailableUpdate
import dev.proflyder.home.feature.update.UpdateCenter
import dev.proflyder.home.feature.update.UpdateFailureKind
import dev.proflyder.home.feature.update.UpdatePhase
import dev.proflyder.home.feature.update.UpdateState
import dev.proflyder.home.feature.update.impl.R
import dev.proflyder.home.feature.update.impl.databinding.DialogUpdatePromptBinding
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLooper

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
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
            setTheme(com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar)
            super.onCreate(savedInstanceState)
        }
    }

    @Test
    fun theAvailableReleaseShowsVersionNotesAndTheUpdateAction() {
        val themed = theme()
        val binding = DialogUpdatePromptBinding.inflate(LayoutInflater.from(themed))
        val state = UpdateState(
            currentVersion = "0.1.0-old",
            phase = UpdatePhase.AVAILABLE,
            available = AvailableUpdate("0.2.0-abc1234", 2L, "Alarms: silent cold starts\nChat: attach menu"),
            promptVisible = true,
        )

        UpdatePromptContent(themed, binding, FakeCenter(state)).render(state)

        assertEquals(View.VISIBLE, binding.updateNotes.visibility)
        assertEquals(themed.getString(R.string.home_update_action_update), binding.updateAction.text.toString())
        assertEquals(View.VISIBLE, binding.updateLater.visibility)
        savePng(binding.root, "update-prompt-available-dark.png")
    }

    @Test
    fun theDownloadingStateShowsDeterminateProgressWithoutActions() {
        val themed = theme()
        val binding = DialogUpdatePromptBinding.inflate(LayoutInflater.from(themed))
        val state = UpdateState(
            currentVersion = "0.1.0-old",
            phase = UpdatePhase.DOWNLOADING,
            available = AvailableUpdate("0.2.0-abc1234", 2L, "Alarms: silent cold starts"),
            progressPercent = 42,
            promptVisible = true,
        )

        UpdatePromptContent(themed, binding, FakeCenter(state)).render(state)

        assertEquals(View.VISIBLE, binding.updateProgress.visibility)
        assertFalse(binding.updateProgress.isIndeterminate)
        assertEquals(42, binding.updateProgress.progress)
        assertEquals(View.GONE, binding.updateAction.visibility)
        assertEquals(View.GONE, binding.updateLater.visibility)
        savePng(binding.root, "update-prompt-downloading-dark.png")
    }

    @Test
    fun theFirstTesterSignInIsExplained() {
        val themed = theme()
        val binding = DialogUpdatePromptBinding.inflate(LayoutInflater.from(themed))
        val state = UpdateState(
            currentVersion = "0.1.0-old",
            phase = UpdatePhase.SIGN_IN_REQUIRED,
            failure = UpdateFailureKind.SIGN_IN_REQUIRED,
            promptVisible = true,
        )

        UpdatePromptContent(themed, binding, FakeCenter(state)).render(state)

        assertEquals(View.VISIBLE, binding.updateMessage.visibility)
        assertEquals(themed.getString(R.string.home_update_sign_in_explanation), binding.updateMessage.text.toString())
        assertEquals(themed.getString(R.string.home_update_action_sign_in), binding.updateAction.text.toString())
        savePng(binding.root, "update-prompt-sign-in-dark.png")
    }

    private fun theme(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
    )

    private fun savePng(root: View, name: String) {
        val activity = Robolectric.buildActivity(DialogHostActivity::class.java).setup().get()
        val container = FrameLayout(activity)
        container.addView(root, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        activity.setContentView(container)
        ShadowLooper.idleMainLooper()
        val width = (CARD_WIDTH_DP * root.resources.displayMetrics.density).toInt()
        val maxHeight = (CARD_MAX_HEIGHT_DP * root.resources.displayMetrics.density).toInt()
        container.measure(exactly(width), View.MeasureSpec.makeMeasureSpec(maxHeight, View.MeasureSpec.AT_MOST))
        container.layout(0, 0, container.measuredWidth, container.measuredHeight)
        assertTrue(root.height > 0)
        val bitmap = Bitmap.createBitmap(container.measuredWidth, container.measuredHeight, Bitmap.Config.ARGB_8888)
        container.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val CARD_WIDTH_DP = 360
        const val CARD_MAX_HEIGHT_DP = 880
    }
}

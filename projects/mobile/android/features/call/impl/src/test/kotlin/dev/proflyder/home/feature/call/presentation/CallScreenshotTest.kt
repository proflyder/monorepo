package dev.proflyder.home.feature.call.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.call.CallFailure
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.impl.databinding.ActivityCallBinding
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
internal class CallScreenshotTest {
    private val active = CallScreenState(status = CallStatus.Active(754), live = true, muted = true, proximityLock = true)

    @Test
    @Config(qualifiers = "notnight")
    fun activeLight() {
        val binding = render(active)
        val context = binding.root.context
        assertEquals(context.getString(R.string.call_status_active_timer, "12:34"), binding.callStatus.text.toString())
        assertEquals(context.getString(R.string.call_unmute), binding.callMute.text.toString())
        assertTrue(binding.callMute.isEnabled)
        save(binding.root, "call-active-light.png")
    }

    @Test
    @Config(qualifiers = "night")
    fun activeDark() = save(render(active).root, "call-active-dark.png")

    @Test
    @Config(qualifiers = "night")
    fun endedWithFailureDark() {
        val ended = CallScreenState(status = CallStatus.Ended(IntercomEndReason.UNREACHABLE, CallFailure.StationOffline))
        val binding = render(ended)
        val context = binding.root.context
        assertEquals(context.getString(R.string.call_close), binding.callHangup.text.toString())
        assertEquals(View.VISIBLE, binding.callError.visibility)
        assertFalse(binding.callMute.isEnabled)
        save(binding.root, "call-ended-dark.png")
    }

    @Test
    @Config(qualifiers = "notnight")
    fun ringingLight() {
        val binding = render(CallScreenState(status = CallStatus.Ringing, live = true))
        assertEquals(binding.root.context.getString(R.string.call_hangup), binding.callHangup.text.toString())
        save(binding.root, "call-ringing-light.png")
    }

    private fun render(state: CallScreenState): ActivityCallBinding {
        val context = themed()
        val binding = ActivityCallBinding.inflate(LayoutInflater.from(context))
        binding.root.setBackgroundColor(MaterialColors.getColor(context, com.google.android.material.R.attr.colorSurface, 0))
        binding.callName.text = context.getString(R.string.call_station_name)
        CallScreen(binding, CallTexts(context)).render(state)
        return binding
    }

    private fun themed(): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, AppTheme.EMERALD.overlayRes)
    }

    private fun save(root: View, name: String) {
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
    }
}

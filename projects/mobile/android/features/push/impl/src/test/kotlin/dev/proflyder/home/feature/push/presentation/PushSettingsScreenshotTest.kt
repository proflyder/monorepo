package dev.proflyder.home.feature.push.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.FragmentPushSettingsBinding
import java.io.File
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PushSettingsScreenshotTest {
    @Test
    fun missingPermissionsAreVisibleInTheDarkTheme() = renderSettings("push-settings-dark.png")

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun missingPermissionsAreVisibleInTheLightTheme() = renderSettings("push-settings-light.png")

    private fun renderSettings(name: String) {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        themed.theme.applyStyle(dev.proflyder.home.core.theme.R.style.ThemeOverlay_Home_Neon, true)
        val binding = FragmentPushSettingsBinding.inflate(LayoutInflater.from(themed))
        val missing = themed.getString(R.string.push_permission_missing)
        binding.pushNotificationsRow.subtitle = missing
        binding.pushDndRow.subtitle = missing
        binding.pushFullScreenRow.subtitle = missing
        binding.pushRingSummary.text = themed.getString(R.string.push_ring_incomplete)
        binding.pushDeviceName.text = SAMPLE_DEVICE_NAME
        binding.pushRegistration.text = themed.getString(R.string.push_not_registered)
        savePng(binding.root, name)
        assertNull(binding.pushNotificationsRow.background)
    }

    private fun savePng(root: View, name: String) {
        val metrics = root.resources.displayMetrics
        val width = (411 * metrics.density).toInt()
        val height = (891 * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)
}

private const val SAMPLE_DEVICE_NAME = "Stas"

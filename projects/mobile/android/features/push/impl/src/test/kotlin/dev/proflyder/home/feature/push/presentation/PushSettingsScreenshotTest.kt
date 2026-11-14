package dev.proflyder.home.feature.push.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.FragmentPushSettingsBinding
import java.io.File
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The permission screen is real Material UI: only an inflated layout proves the
 * rows, the button labels and the ring summary render in the dark theme. The
 * PNG is written to Bazel's undeclared outputs as review evidence.
 *
 * The state shown is the one the owner sees before granting anything, which is
 * the case that must be understandable: three "not allowed" rows, the phone
 * name field and an honest warning that the loud ring will not start.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PushSettingsScreenshotTest {
    @Test
    fun missingPermissionsAreVisibleInTheDarkTheme() {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        val binding = FragmentPushSettingsBinding.inflate(LayoutInflater.from(themed))
        val missing = themed.getString(R.string.push_permission_missing)
        binding.pushNotificationsStatus.text = missing
        binding.pushDndStatus.text = missing
        binding.pushFullScreenStatus.text = missing
        binding.pushNotificationsAction.text = themed.getString(R.string.push_permission_grant)
        binding.pushDndAction.text = themed.getString(R.string.push_permission_grant)
        binding.pushFullScreenAction.text = themed.getString(R.string.push_permission_grant)
        binding.pushRingSummary.text = themed.getString(R.string.push_ring_incomplete)
        binding.pushDeviceName.setText(themed.getString(R.string.push_device_name_sample))
        binding.pushDeviceNameSave.isEnabled = true
        binding.pushRegistration.text = themed.getString(R.string.push_not_registered)
        savePng(binding.root, "push-settings-dark.png")
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

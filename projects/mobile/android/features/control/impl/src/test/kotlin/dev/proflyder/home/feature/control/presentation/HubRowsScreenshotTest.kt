package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.ColorRole
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubRowBinding
import java.io.File
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HubRowsScreenshotTest {
    @Test
    @Config(qualifiers = "notnight")
    fun hubRowsLight() = check("light")

    @Test
    @Config(qualifiers = "night")
    fun hubRowsDark() = check("dark")

    private fun check(mode: String) {
        val context = themed()
        val hub = ItemControlHubBinding.inflate(LayoutInflater.from(context))
        val rows = SAMPLE_ROWS.map { (time, label) -> row(hub, time, label) }
        rows.forEach { hub.controlHubRows.addView(it.root) }
        rows.first().hubRowAction.visibility = View.VISIBLE
        rows.first().hubRowAction.setImageResource(R.drawable.control_ic_pause)
        val surface = FrameLayout(context).apply {
            setBackgroundColor(MaterialColors.getColor(context, ColorRole.SURFACE.attr, 0))
            val padding = resources.getDimensionPixelSize(dev.proflyder.home.core.designsystem.R.dimen.ds_space_l)
            setPadding(padding, padding, padding, padding)
            addView(hub.root)
        }
        save(layout(surface), "control-hub-rows-$mode.png")
        rows.forEach { assertNotNull(it.root.foreground) }
    }

    private fun row(hub: ItemControlHubBinding, time: String, label: String): ItemControlHubRowBinding =
        ItemControlHubRowBinding.inflate(LayoutInflater.from(hub.root.context), hub.controlHubRows, false).apply {
            hubRowIcon.setImageResource(R.drawable.control_ic_timer)
            hubRowTime.text = time
            hubRowLabel.text = label
            root.setOnClickListener {}
        }

    private fun themed(): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, AppTheme.NEON.overlayRes)
    }

    private fun layout(view: View): View {
        val metrics = view.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        view.layout(0, 0, width, view.measuredHeight)
        return view
    }

    private fun save(view: View, name: String) {
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        const val WIDTH_DP = 411
        const val PNG_QUALITY = 100
        val SAMPLE_ROWS = listOf("04:59" to "Таймер чая", "19:30" to "Полить цветы", "завтра 08:00" to "Вынести мусор")
    }
}

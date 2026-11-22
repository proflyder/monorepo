package dev.proflyder.home.core.designsystem

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.widget.FrameLayout
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.LightParams
import dev.proflyder.home.core.model.LightRanges
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LightParamsEditorScreenshotTest {
    private val controls = LightControls(
        brightness = true,
        colorTemp = true,
        color = false,
        minKelvin = 2200,
        maxKelvin = 6500,
        effects = listOf("candle", "rainbow"),
    )

    private val state = LightParamsEditorState(
        ranges = LightRanges.of(controls),
        effects = controls.effects,
        params = LightParams(brightnessPct = 40, kelvin = 3000, effect = "candle"),
        transitionText = "2",
    )

    @Test
    fun rendersStateAndOnlySendsEdits() {
        val edits = mutableListOf<LightParamsEdit>()
        val context = themed(AppTheme.entries.first())
        val editor = LightParamsEditor(context) { edits += it }
        editor.render(state)
        save(editor.root, "light_params_editor.png")
        assertEquals(emptyList<LightParamsEdit>(), edits.filterNot { it is LightParamsEdit.Transition })
        assertEquals(3000, state.apply(LightParamsEdit.Kelvin(3020)).params.kelvin)
    }

    private fun save(view: View, name: String) {
        val host = FrameLayout(view.context).apply { addView(view) }
        host.setBackgroundColor(color(view.context, com.google.android.material.R.attr.colorSurface))
        val width = View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY)
        host.measure(width, View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED))
        host.layout(0, 0, host.measuredWidth, host.measuredHeight)
        val bitmap = Bitmap.createBitmap(host.measuredWidth, host.measuredHeight, Bitmap.Config.ARGB_8888)
        host.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        const val WIDTH = 1080
        const val PNG_QUALITY = 100
    }
}

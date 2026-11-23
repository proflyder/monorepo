package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import com.google.android.material.slider.Slider
import dev.proflyder.home.core.designsystem.GradientSlider
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.ColorRole
import dev.proflyder.home.core.theme.ColorScheme
import dev.proflyder.home.feature.control.impl.R
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import com.google.android.material.R as MaterialR
import dev.proflyder.home.core.designsystem.R as DesignSystemR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
internal class LightSheetScreenshotTest {
    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
    fun emeraldLight() = check(AppTheme.EMERALD, LIGHT)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun emeraldDark() = check(AppTheme.EMERALD, DARK)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
    fun systemLight() = check(AppTheme.SYSTEM, LIGHT)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun systemDark() = check(AppTheme.SYSTEM, DARK)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
    fun neonLight() = check(AppTheme.NEON, LIGHT)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun neonDark() = check(AppTheme.NEON, DARK)

    private fun check(theme: AppTheme, mode: String) {
        val context = themed(theme)
        val content = LightSheet(context, entry()) {}.content
        content.setBackgroundColor(sheetColor(context))
        val width = (WIDTH_DP * context.resources.displayMetrics.density).toInt()
        content.measure(exactly(width), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED))
        content.layout(0, 0, width, content.measuredHeight)
        val scheme = if (mode == DARK) theme.palette.dark else theme.palette.light
        assertThemedSlider(content.findViewById(R.id.light_brightness), scheme)
        assertThemedSlider(content.findViewById(R.id.light_saturation), scheme)
        assertGradientSlider(content.findViewById(R.id.light_temp))
        assertGradientSlider(content.findViewById(R.id.light_hue))
        assertBlockGap(content, context)
        save(content, "light-sheet-${theme.key}-$mode.png")
    }

    private fun assertThemedSlider(slider: Slider, scheme: ColorScheme) {
        val enabled = intArrayOf(android.R.attr.state_enabled)
        assertEquals(scheme[ColorRole.PRIMARY], slider.trackActiveTintList.getColorForState(enabled, 0))
        assertEquals(scheme[ColorRole.SURFACE_CONTAINER_HIGHEST], slider.trackInactiveTintList.getColorForState(enabled, 0))
        assertEquals(scheme[ColorRole.ON_PRIMARY], slider.tickActiveTintList.getColorForState(enabled, 0))
        assertEquals(scheme[ColorRole.ON_SURFACE_VARIANT], slider.tickInactiveTintList.getColorForState(enabled, 0))
        assertEquals(scheme[ColorRole.PRIMARY], slider.thumbTintList.getColorForState(enabled, 0))
        assertEquals(0, slider.trackStopIndicatorSize)
    }

    private fun assertGradientSlider(slider: GradientSlider) {
        assertEquals(Color.TRANSPARENT, slider.trackActiveTintList.defaultColor)
        assertEquals(Color.TRANSPARENT, slider.trackInactiveTintList.defaultColor)
        assertEquals(HIDDEN_TICKS, slider.tickVisibilityMode)
    }

    private fun assertBlockGap(content: View, context: Context) {
        val scale = content.findViewById<View>(R.id.light_temp_scale)
        val colorGroup = content.findViewById<View>(R.id.light_color_group)
        val minGap = context.resources.getDimensionPixelSize(DesignSystemR.dimen.ds_space_l)
        val gap = top(colorGroup, content) - (top(scale, content) + scale.height)
        assertTrue("gap $gap < $minGap", gap >= minGap)
    }

    private fun top(view: View, root: View): Int = if (view === root) 0 else view.top + top(view.parent as View, root)

    private fun entry() = ControlEntry(
        id = "light.lamp",
        kind = ControlKind.LIGHT,
        title = "Лампа",
        room = "Спальня",
        icon = "lightbulb",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.ON,
        available = true,
        light = LightControls(
            brightness = true,
            colorTemp = true,
            color = true,
            minKelvin = 2200,
            maxKelvin = 6500,
            brightnessPct = 100,
            kelvin = 2800,
            hue = 40,
            saturation = 0,
            mode = "colorTemp",
        ),
    )

    private fun sheetColor(context: Context): Int =
        MaterialColors.getColor(context, MaterialR.attr.colorSurfaceContainerLow, 0)

    private fun themed(theme: AppTheme): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            DesignSystemR.style.Base_Theme_Home,
        )
        base.theme.applyStyle(theme.overlayRes, true)
        return base
    }

    private fun save(view: View, name: String) {
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH_DP = 411
        const val PNG_QUALITY = 100
        const val HIDDEN_TICKS = 2
        const val LIGHT = "light"
        const val DARK = "dark"
    }
}

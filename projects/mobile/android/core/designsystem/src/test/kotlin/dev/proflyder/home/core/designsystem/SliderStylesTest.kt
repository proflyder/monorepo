package dev.proflyder.home.core.designsystem

import android.content.Context
import android.content.res.TypedArray
import android.graphics.Color
import android.util.TypedValue
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.ColorRole
import dev.proflyder.home.core.theme.ColorScheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-notnight-xxhdpi")
class SliderStylesTest {
    @Test
    fun themeDefaultPointsAtTheDesignSystemSlider() = AppTheme.entries.forEach(::assertDefaultStyle)

    @Test
    fun lightPaletteDrivesTracksAndTicks() = AppTheme.entries.forEach { assertPalette(it, it.palette.light) }

    @Test
    @Config(qualifiers = "w411dp-h891dp-night-xxhdpi")
    fun darkPaletteDrivesTracksAndTicks() = AppTheme.entries.forEach { assertPalette(it, it.palette.dark) }

    @Test
    fun gradientStyleReplacesTheTrackWithTheGradient() {
        val context = themed(AppTheme.NEON)
        val colors = styleColors(context, R.style.Widget_Home_Slider_Gradient)
        assertEquals(Color.TRANSPARENT, colors[TRACK_ACTIVE])
        assertEquals(Color.TRANSPARENT, colors[TRACK_INACTIVE])
        assertEquals(TICKS_HIDDEN, styleInt(context, R.style.Widget_Home_Slider_Gradient, MaterialR.attr.tickVisibilityMode))
    }

    private fun assertDefaultStyle(theme: AppTheme) {
        val value = TypedValue()
        themed(theme).theme.resolveAttribute(MaterialR.attr.sliderStyle, value, true)
        assertEquals(theme.key, R.style.Widget_Home_Slider, value.resourceId)
    }

    private fun assertPalette(theme: AppTheme, scheme: ColorScheme) {
        val colors = styleColors(themed(theme), R.style.Widget_Home_Slider)
        assertEquals(theme.key, scheme[ColorRole.PRIMARY], colors[TRACK_ACTIVE])
        assertEquals(theme.key, scheme[ColorRole.SURFACE_CONTAINER_HIGHEST], colors[TRACK_INACTIVE])
        assertEquals(theme.key, scheme[ColorRole.ON_PRIMARY], colors[TICK_ACTIVE])
        assertEquals(theme.key, scheme[ColorRole.ON_SURFACE_VARIANT], colors[TICK_INACTIVE])
        assertEquals(theme.key, scheme[ColorRole.PRIMARY], colors[THUMB])
        assertEquals(theme.key, 0, styleDimension(themed(theme), R.style.Widget_Home_Slider, MaterialR.attr.trackStopIndicatorSize))
        assertTrue(theme.key, styleDimension(themed(theme), R.style.Widget_Home_Slider, MaterialR.attr.tickRadiusActive) > 0)
    }

    private fun styleColors(context: Context, style: Int): Map<String, Int> =
        context.obtainStyledAttributes(style, COLOR_ATTRS).use(::readColors)

    private fun readColors(attrs: TypedArray): Map<String, Int> = mapOf(
        TRACK_ACTIVE to attrs.colorAt(0),
        TRACK_INACTIVE to attrs.colorAt(1),
        TICK_ACTIVE to attrs.colorAt(2),
        TICK_INACTIVE to attrs.colorAt(3),
        THUMB to attrs.colorAt(4),
    )

    private fun TypedArray.colorAt(index: Int): Int = requireNotNull(getColorStateList(index)).defaultColor

    private fun styleInt(context: Context, style: Int, attr: Int): Int =
        context.obtainStyledAttributes(style, intArrayOf(attr)).use { it.getInt(0, 0) }

    private fun styleDimension(context: Context, style: Int, attr: Int): Int =
        context.obtainStyledAttributes(style, intArrayOf(attr)).use { it.getDimensionPixelSize(0, 0) }

    private companion object {
        const val TRACK_ACTIVE = "trackActive"
        const val TRACK_INACTIVE = "trackInactive"
        const val TICK_ACTIVE = "tickActive"
        const val TICK_INACTIVE = "tickInactive"
        const val THUMB = "thumb"
        const val TICKS_HIDDEN = 2
        val COLOR_ATTRS = intArrayOf(
            MaterialR.attr.trackColorActive,
            MaterialR.attr.trackColorInactive,
            MaterialR.attr.tickColorActive,
            MaterialR.attr.tickColorInactive,
            MaterialR.attr.thumbColor,
        )
    }
}

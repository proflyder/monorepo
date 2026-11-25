package dev.proflyder.home.core.designsystem

import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.RippleDrawable
import android.view.View
import androidx.core.content.ContextCompat
import dev.proflyder.home.core.designsystem.list.GroupSlot
import dev.proflyder.home.core.designsystem.list.GroupedRowRipple
import dev.proflyder.home.core.theme.AppTheme
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
class RowRippleTest {
    @Test
    fun theRowRippleUsesTheThemeAccentInEveryTheme() = AppTheme.entries.forEach(::assertTokens)

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun theRowRippleUsesTheThemeAccentInLightThemes() = AppTheme.entries.forEach(::assertTokens)

    @Test
    fun theRippleMaskFollowsTheGroupedRowCorners() {
        val view = View(themed(AppTheme.EMERALD)).apply { isClickable = true }
        val ripple = GroupedRowRipple(view, RADIUS)

        ripple.bind(GroupSlot(first = true, last = false))
        assertTrue(view.foreground is RippleDrawable)
        assertCorners(ripple, top = RADIUS, bottom = 0f)

        ripple.bind(GroupSlot(first = false, last = false))
        assertCorners(ripple, top = 0f, bottom = 0f)

        ripple.bind(GroupSlot(first = false, last = true))
        assertCorners(ripple, top = 0f, bottom = RADIUS)
    }

    @Test
    fun aSlotChangeSwapsTheRippleShape() {
        val view = View(themed(AppTheme.EMERALD)).apply { isClickable = true }
        val ripple = GroupedRowRipple(view, RADIUS)

        ripple.bind(GroupSlot(first = true, last = true))
        val single = view.foreground
        ripple.bind(GroupSlot(first = false, last = false))

        assertTrue(single !== view.foreground)
        assertCorners(ripple, top = 0f, bottom = 0f)
    }

    private fun assertCorners(ripple: GroupedRowRipple, top: Float, bottom: Float) {
        val model = checkNotNull(ripple.mask).shapeAppearanceModel
        val bounds = android.graphics.RectF(0f, 0f, MASK_SIZE, MASK_SIZE)
        assertEquals(top, model.topLeftCornerSize.getCornerSize(bounds), 0.5f)
        assertEquals(top, model.topRightCornerSize.getCornerSize(bounds), 0.5f)
        assertEquals(bottom, model.bottomLeftCornerSize.getCornerSize(bounds), 0.5f)
        assertEquals(bottom, model.bottomRightCornerSize.getCornerSize(bounds), 0.5f)
    }

    @Test
    fun aRowThatIsNotClickableHasNoRipple() {
        val view = View(themed(AppTheme.EMERALD))
        val ripple = GroupedRowRipple(view, RADIUS)

        ripple.bind(GroupSlot(first = true, last = true))

        assertNull(view.foreground)
        assertFalse(view.isClickable)
    }

    private fun assertTokens(theme: AppTheme) {
        val context = themed(theme)
        val primary = color(context, MaterialR.attr.colorPrimary)
        val states = requireNotNull(ContextCompat.getColorStateList(context, R.color.ds_row_item_ripple))

        assertAccentAt(states.pressed(), primary, PRESSED_ALPHA)
        assertAccentAt(states.hovered(), primary, HOVER_ALPHA)
        assertTrue(Color.alpha(color(context, R.attr.homeRowItemRippleColor)) < MAX_ALPHA)
    }

    private fun ColorStateList.pressed(): Int = getColorForState(intArrayOf(android.R.attr.state_pressed), Color.TRANSPARENT)

    private fun ColorStateList.hovered(): Int = getColorForState(intArrayOf(android.R.attr.state_hovered), Color.TRANSPARENT)

    private fun assertAccentAt(actual: Int, accent: Int, alpha: Float) {
        assertEquals(Color.red(accent), Color.red(actual))
        assertEquals(Color.green(accent), Color.green(actual))
        assertEquals(Color.blue(accent), Color.blue(actual))
        assertTrue(abs(Color.alpha(actual) - alpha * MAX_ALPHA) <= ALPHA_TOLERANCE)
    }

    private companion object {
        const val RADIUS = 16f
        const val MASK_SIZE = 100f
        const val PRESSED_ALPHA = 0.2f
        const val HOVER_ALPHA = 0.08f
        const val MAX_ALPHA = 255
        const val ALPHA_TOLERANCE = 4
    }
}

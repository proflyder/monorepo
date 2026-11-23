package dev.proflyder.home.core.designsystem

import android.content.Context
import android.view.View
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.theme.AppTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
class ButtonStylesTest {
    @Test
    fun everyVariantHasTheSizeHeight() = AppTheme.entries.forEach(::assertHeights)

    @Test
    fun loadingKeepsTheWidth() {
        val button = button(themed(AppTheme.EMERALD), "Widget.Home.Button.Filled.M")
        button.setText(R.string.ds_catalog_action)
        val before = measure(button).measuredWidth
        button.setLoading(true)
        assertEquals(before, measure(button).measuredWidth)
        assertTrue(button.isLoading)
        button.setLoading(false)
        assertEquals(button.resources.getString(R.string.ds_catalog_action), button.text.toString())
    }

    @Test
    fun destructiveUsesColorError() = AppTheme.entries.forEach(::assertDestructive)

    @Test
    fun iconButtonsKeepTheTouchTarget() = ICON_VARIANTS.forEach(::assertIconButtons)

    private fun assertHeights(theme: AppTheme) = BUTTON_VARIANTS.forEach { variant ->
        SIZES.forEach { (size, dp) -> assertHeight(theme, variant, size, dp) }
    }

    private fun assertDestructive(theme: AppTheme) {
        val context = themed(theme)
        val error = color(context, MaterialR.attr.colorError)
        SIZES.keys.forEach { size -> assertDestructive(button(context, "Widget.Home.Button.Destructive.$size"), error) }
    }

    private fun assertDestructive(button: MaterialButton, error: Int) {
        assertEquals("text", error, button.currentTextColor)
        assertEquals("stroke", error, button.strokeColor.defaultColor)
        assertTrue("stroke width", button.strokeWidth > 0)
    }

    private fun assertIconButtons(variant: String) = SIZES.forEach { (size, dp) -> assertIconButton(variant, size, dp) }

    private fun assertIconButton(variant: String, size: String, dp: Int) {
        val button = measure(button(themed(AppTheme.NEON), "Widget.Home.IconButton.$variant.$size"))
        val density = button.resources.displayMetrics.density
        assertEquals("$variant $size touch", (TOUCH_DP * density).toInt(), button.measuredHeight)
        val style = R.style::class.java.getField("Widget_Home_IconButton_${variant}_$size").getInt(null)
        val inset = button.context.obtainStyledAttributes(style, intArrayOf(android.R.attr.insetTop)).use { it.getDimensionPixelSize(0, -1) }
        assertEquals("$variant $size visible", (dp * density).toInt(), button.measuredHeight - 2 * inset)
    }

    private fun assertHeight(theme: AppTheme, variant: String, size: String, dp: Int) {
        val button = measure(button(themed(theme), "Widget.Home.Button.$variant.$size").apply { setText(R.string.ds_catalog_action) })
        val expected = (dp * button.resources.displayMetrics.density).toInt()
        assertEquals("$theme $variant.$size", expected, button.measuredHeight)
    }

    private fun button(context: Context, style: String): MaterialButton {
        val res = R.style::class.java.getField(style.replace('.', '_')).getInt(null)
        val button = MaterialButton(ContextThemeWrapper(context, res), null, 0)
        val height = context.obtainStyledAttributes(res, intArrayOf(android.R.attr.layout_height)).use { it.getLayoutDimension(0, 0) }
        FrameLayout(context).addView(button, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, height))
        return button
    }

    private fun measure(view: View): View {
        val spec = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)
        val height = when (val lp = view.layoutParams.height) {
            in 1..Int.MAX_VALUE -> View.MeasureSpec.makeMeasureSpec(lp, View.MeasureSpec.EXACTLY)
            else -> spec
        }
        view.measure(spec, height)
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
        return view
    }

    private companion object {
        const val TOUCH_DP = 48
        val ICON_VARIANTS = listOf("Standard", "Filled", "Tonal", "Outlined")
    }
}

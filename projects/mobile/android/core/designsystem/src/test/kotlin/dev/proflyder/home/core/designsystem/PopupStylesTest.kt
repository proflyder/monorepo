package dev.proflyder.home.core.designsystem

import android.content.Context
import android.content.res.XmlResourceParser
import android.graphics.Color
import android.view.LayoutInflater
import android.widget.ImageView
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.graphics.ColorUtils
import dev.proflyder.home.core.theme.AppTheme
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmlpull.v1.XmlPullParser
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
class PopupStylesTest {
    @Test
    fun popupTokensApplyToEveryTheme() = AppTheme.entries.forEach(::assertTokens)

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun popupTokensStayDistinctInLightThemes() = AppTheme.entries.forEach(::assertTokens)

    private fun assertTokens(theme: AppTheme) {
        val context = themed(theme)
        val surface = color(context, R.attr.homePopupSurface)
        val card = color(context, MaterialR.attr.colorSurfaceContainer)
        val highest = color(context, MaterialR.attr.colorSurfaceContainerHighest)
        val primary = color(context, MaterialR.attr.colorPrimary)
        val onSurface = color(context, MaterialR.attr.colorOnSurface)
        val stroke = color(context, R.attr.homePopupStroke)

        assertEquals(highest, surface)
        assertNotEquals(card, surface)
        assertTrue(abs(ColorUtils.calculateLuminance(surface) - ColorUtils.calculateLuminance(card)) > MIN_LUMINANCE_GAP)
        assertAccentAt(stroke, primary, STROKE_ALPHA)
        assertAccentAt(ContextCompat.getColor(context, R.color.ds_popup_stroke), primary, STROKE_ALPHA)

        val cardXml = R.drawable.ds_bg_popup_menu
        assertEquals(R.attr.homePopupSurface, resource(context, cardXml, "solid", "color"))
        assertEquals(R.attr.homePopupStroke, resource(context, cardXml, "stroke", "color"))
        assertEquals(R.dimen.ds_popup_stroke_width, resource(context, cardXml, "stroke", "width"))
        assertEquals(R.dimen.ds_radius_xl, resource(context, cardXml, "corners", "radius"))

        val row = LayoutInflater.from(context).inflate(R.layout.ds_popup_menu_item, null)
        assertEquals(primary, row.findViewById<ImageView>(R.id.ds_popup_menu_icon).imageTintList?.defaultColor)
        assertEquals(onSurface, row.findViewById<TextView>(R.id.ds_popup_menu_label).currentTextColor)
        assertEquals(R.attr.homePopupItemRippleColor, resource(context, R.drawable.ds_bg_popup_menu_item, "ripple", "color"))
        val pressed = requireNotNull(ContextCompat.getColorStateList(context, R.color.ds_popup_item_ripple))
            .getColorForState(intArrayOf(android.R.attr.state_pressed), Color.TRANSPARENT)
        assertAccentAt(pressed, primary, RIPPLE_ALPHA)
    }

    private fun resource(context: Context, drawable: Int, tag: String, attribute: String): Int =
        context.resources.getXml(drawable).use { parser -> firstReference(parser, tag, attribute) }

    private fun firstReference(parser: XmlResourceParser, tag: String, attribute: String): Int {
        while (parser.next() != XmlPullParser.END_DOCUMENT) {
            val id = tagReference(parser, tag, attribute)
            if (id != 0) return id
        }
        return 0
    }

    private fun tagReference(parser: XmlResourceParser, tag: String, attribute: String): Int {
        if (parser.eventType != XmlPullParser.START_TAG || parser.name != tag) return 0
        val index = (0 until parser.attributeCount)
            .firstOrNull { parser.getAttributeName(it) == attribute } ?: return 0
        return reference(parser, index)
    }

    private fun reference(parser: XmlResourceParser, index: Int): Int {
        val raw = parser.getAttributeValue(index) ?: return 0
        return when {
            raw.startsWith("?") -> raw.drop(1).toIntOrNull() ?: 0
            else -> parser.getAttributeResourceValue(index, 0)
        }
    }

    private fun assertAccentAt(actual: Int, accent: Int, alpha: Float) {
        assertEquals(Color.red(accent), Color.red(actual))
        assertEquals(Color.green(accent), Color.green(actual))
        assertEquals(Color.blue(accent), Color.blue(actual))
        assertTrue(abs(Color.alpha(actual) - alpha * MAX_ALPHA) <= ALPHA_TOLERANCE)
    }

    private companion object {
        const val STROKE_ALPHA = 0.5f
        const val RIPPLE_ALPHA = 0.2f
        const val MAX_ALPHA = 255
        const val ALPHA_TOLERANCE = 4
        const val MIN_LUMINANCE_GAP = 0.01
    }
}

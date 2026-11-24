package dev.proflyder.home.core.designsystem

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.Gravity
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import com.google.android.material.card.MaterialCardView
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PopupScreenshotTest {
    @Test
    fun emeraldDark() = shoot(AppTheme.EMERALD, "dark")

    @Test
    fun systemDark() = shoot(AppTheme.SYSTEM, "dark")

    @Test
    fun neonDark() = shoot(AppTheme.NEON, "dark")

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun emeraldLight() = shoot(AppTheme.EMERALD, "light")

    private fun shoot(theme: AppTheme, mode: String) {
        val context = themed(theme)
        val root = screen(context)
        layout(root)
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, "popup-${theme.key}-$mode-over-cards.png").outputStream()
            .use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun screen(context: android.content.Context): View {
        val root = FrameLayout(context).apply {
            setBackgroundColor(color(context, MaterialR.attr.colorSurface))
        }
        val column = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(context, 16), dp(context, 88), dp(context, 16), 0)
        }
        root.addView(column, matchParent())
        CARD_TITLES.forEach { title ->
            column.addView(card(context, title), cardParams(context))
        }
        root.addView(title(context), FrameLayout.LayoutParams(MATCH_PARENT, dp(context, 64)))
        root.addView(menu(context), popupParams(context))
        return root
    }

    private fun title(context: android.content.Context): View = LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(dp(context, 16), 0, dp(context, 16), 0)
        addView(TextView(context).apply {
            text = "Дом"
            setTextAppearance(R.style.TextAppearance_Home_ToolbarTitle)
        }, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
        addView(TextView(context).apply {
            text = "⋮"
            setTextAppearance(R.style.TextAppearance_Home_ToolbarTitle)
        })
    }

    private fun card(context: android.content.Context, title: String): View = MaterialCardView(context).apply {
        radius = context.resources.getDimension(R.dimen.ds_radius_xl)
        cardElevation = 0f
        strokeWidth = 0
        setCardBackgroundColor(color(context, MaterialR.attr.colorSurfaceContainer))
        addView(LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(context, 16), dp(context, 16), dp(context, 16), dp(context, 16))
            addView(TextView(context).apply {
                text = title
                setTextAppearance(R.style.TextAppearance_Home_ToolbarTitle)
            })
            addView(TextView(context).apply {
                text = "Включено"
                setTextAppearance(R.style.TextAppearance_Home_ChatMeta)
            })
        })
    }

    private fun menu(context: android.content.Context): View {
        val inflater = LayoutInflater.from(context)
        val list = inflater.inflate(R.layout.ds_popup_menu, null) as LinearLayout
        list.addView(row(inflater, list, R.drawable.ds_ic_check, "Позвонить"))
        list.addView(row(inflater, list, R.drawable.ds_ic_add, "Настройки"))
        return list
    }

    private fun row(inflater: LayoutInflater, parent: ViewGroup, iconRes: Int, label: String): View =
        inflater.inflate(R.layout.ds_popup_menu_item, parent, false).apply {
            findViewById<ImageView>(R.id.ds_popup_menu_icon).setImageResource(iconRes)
            findViewById<TextView>(R.id.ds_popup_menu_label).text = label
        }

    private fun layout(root: View) {
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
    }

    private fun matchParent(): FrameLayout.LayoutParams =
        FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)

    private fun cardParams(context: android.content.Context): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(MATCH_PARENT, dp(context, 112)).apply { bottomMargin = dp(context, 12) }

    private fun popupParams(context: android.content.Context): FrameLayout.LayoutParams =
        FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT).apply {
            gravity = Gravity.TOP or Gravity.END
            topMargin = dp(context, 56)
            marginEnd = dp(context, 8)
        }

    private fun dp(context: android.content.Context, value: Int): Int =
        (value * context.resources.displayMetrics.density).toInt()

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val PNG_QUALITY = 100
        const val MATCH_PARENT = ViewGroup.LayoutParams.MATCH_PARENT
        const val WRAP_CONTENT = ViewGroup.LayoutParams.WRAP_CONTENT
        val CARD_TITLES = listOf("Яндекс Станция Миди", "Лампа", "Светильник")
    }
}

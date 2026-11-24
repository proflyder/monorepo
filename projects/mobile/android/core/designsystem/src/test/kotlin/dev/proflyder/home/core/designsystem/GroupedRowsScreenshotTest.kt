package dev.proflyder.home.core.designsystem

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Rect
import android.graphics.drawable.RippleDrawable
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.appcompat.content.res.AppCompatResources
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.shape.MaterialShapeDrawable
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.addGroupedRows
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLooper

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GroupedRowsScreenshotTest {
    @Test
    fun pressedRowsKeepTheirCardShapeInTheDarkTheme() = Row.entries.forEach { shoot(it, "dark") }

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun pressedRowsKeepTheirCardShapeInTheLightTheme() = Row.entries.forEach { shoot(it, "light") }

    @Test
    fun aRowThatBecomesFirstAfterARemovalGetsRoundedTopCorners() {
        val screen = Screen(themed(AppTheme.EMERALD))
        screen.render()
        val rows = screen.list.adapter as Rows
        rows.removeFirstRow()
        screen.render()

        val mask = (screen.row(1).foreground as RippleDrawable).findDrawableByLayerId(android.R.id.mask) as MaterialShapeDrawable
        val bounds = android.graphics.RectF(mask.bounds)
        assertNotEquals(0f, mask.shapeAppearanceModel.topLeftCornerSize.getCornerSize(bounds))
    }

    private fun shoot(row: Row, mode: String) {
        val screen = Screen(themed(AppTheme.EMERALD))
        val idle = screen.render()
        val view = screen.row(row.position)
        press(view)
        val pressed = screen.render().also { drawPressedMask(it, view, screen.list) }
        save(pressed, "grouped-row-pressed-${row.name.lowercase()}-$mode.png")

        val x = view.left + screen.list.left
        val y = view.top + screen.list.top
        assertNotEquals(idle.getPixel(x + view.width / 2, y + view.height / 2), pressed.getPixel(x + view.width / 2, y + view.height / 2))
        assertEquals(row.roundTop, idle.getPixel(x + 1, y + 1) == pressed.getPixel(x + 1, y + 1))
        assertEquals(row.roundBottom, idle.getPixel(x + 1, y + view.height - 2) == pressed.getPixel(x + 1, y + view.height - 2))
        assertEquals(idle.getPixel(x + view.width / 2, y - 2), pressed.getPixel(x + view.width / 2, y - 2))
        assertEquals(idle.getPixel(x + view.width / 2, y + view.height + 2), pressed.getPixel(x + view.width / 2, y + view.height + 2))
    }

    private fun press(view: View) {
        view.drawableHotspotChanged(view.width / 2f, view.height / 2f)
        view.isPressed = true
        ShadowLooper.idleMainLooper(PRESS_MILLIS, TimeUnit.MILLISECONDS)
    }

    private fun drawPressedMask(bitmap: Bitmap, view: View, list: View) {
        val ripple = view.foreground as RippleDrawable
        val mask = ripple.findDrawableByLayerId(android.R.id.mask) as MaterialShapeDrawable
        assertEquals(Rect(0, 0, view.width, view.height), mask.bounds)
        val color = checkNotNull(ContextCompat.getColorStateList(view.context, R.color.ds_row_item_ripple))
            .getColorForState(intArrayOf(android.R.attr.state_pressed), Color.TRANSPARENT)
        val left = list.left + view.left
        val top = list.top + view.top
        MaterialShapeDrawable(mask.shapeAppearanceModel).apply {
            fillColor = ColorStateList.valueOf(color)
            setBounds(left, top, left + view.width, top + view.height)
            draw(Canvas(bitmap))
        }
    }

    private fun save(bitmap: Bitmap, name: String) {
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private enum class Row(val position: Int, val roundTop: Boolean, val roundBottom: Boolean) {
        FIRST(1, true, false),
        MIDDLE(2, false, false),
        LAST(3, false, true),
    }

    private class Screen(context: Context) {
        val list = RecyclerView(context).apply {
            layoutManager = LinearLayoutManager(context)
            adapter = Rows()
            addGroupedRows()
            itemAnimator = null
        }
        private val window = FrameLayout(context).apply {
            background = AppCompatResources.getDrawable(context, windowBackground(context))
            val padding = resources.getDimensionPixelSize(R.dimen.ds_space_l)
            setPadding(padding, padding, padding, padding)
            addView(list, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }

        fun row(position: Int): View = checkNotNull(list.findViewHolderForAdapterPosition(position)).itemView

        fun render(): Bitmap {
            window.measure(exactly(WIDTH), exactly(HEIGHT))
            window.layout(0, 0, WIDTH, HEIGHT)
            return Bitmap.createBitmap(WIDTH, HEIGHT, Bitmap.Config.ARGB_8888).also { window.draw(Canvas(it)) }
        }

        private fun windowBackground(context: Context): Int {
            val value = android.util.TypedValue()
            context.theme.resolveAttribute(android.R.attr.windowBackground, value, true)
            return value.resourceId
        }

        private fun exactly(size: Int) = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)
    }

    private class Rows : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position % SECTION == 0

        private var removed = 0

        fun removeFirstRow() {
            removed = 1
            notifyItemRemoved(1)
        }

        override fun getItemCount(): Int = SECTION * SECTIONS - removed

        override fun getItemViewType(position: Int): Int = if (isSectionHeader(position)) HEADER else ROW

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder =
            object : RecyclerView.ViewHolder(if (viewType == HEADER) header(parent) else row(parent)) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as? ListRow)?.title = "Row $position"
            (holder.itemView as? TextView)?.text = "Section ${position / SECTION}"
        }

        private fun header(parent: ViewGroup) = TextView(parent.context, null, 0, R.style.Widget_Home_SectionHeader)

        private fun row(parent: ViewGroup) = ListRow(parent.context).apply {
            layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            val inset = resources.getDimensionPixelSize(R.dimen.ds_space_l)
            setPaddingRelative(inset, 0, inset, 0)
            subtitle = "Subtitle"
            chevron = true
            setOnClickListener {}
        }
    }

    private companion object {
        const val WIDTH = 1233
        const val HEIGHT = 1400
        const val SECTION = 4
        const val SECTIONS = 2
        const val HEADER = 0
        const val ROW = 1
        const val PNG_QUALITY = 100
        const val PRESS_MILLIS = 1_000L
    }
}

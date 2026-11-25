package dev.proflyder.home.core.designsystem

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.StickySectionHeaders
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class StickyHeadersScreenshotTest {
    private val context = themed(AppTheme.entries.first())
    private val screenColor = MaterialColors.getColor(context, android.R.attr.colorBackground, 0)

    @Test
    fun pinnedHeaderCoversPaddingAndRowsMidList() {
        val list = sectionedList(clipToPadding = false)
        list.scrollBy(0, MID_SCROLL)
        val shot = render(list, "sticky-headers-mid-list.png")
        assertNotEquals(CARD, shot.getPixel(1, 1))
        assertNotEquals(CARD, shot.getPixel(shot.width - 2, HEADER_HEIGHT - 2))
        assertEquals(CARD, shot.getPixel(shot.width / 2, HEADER_HEIGHT + PADDING + ROW_HEIGHT / 2))
    }

    @Test
    fun nextHeaderPushesPinnedOneUp() {
        val list = sectionedList(clipToPadding = true)
        list.scrollBy(0, PUSH_SCROLL)
        val shot = render(list, "sticky-headers-push.png")
        assertNotEquals(CARD, shot.getPixel(shot.width / 2, PADDING / 2))
        assertNotEquals(CARD, shot.getPixel(shot.width - 2, PADDING + 1))
    }

    private fun sectionedList(clipToPadding: Boolean) = RecyclerView(context).apply {
        layoutManager = LinearLayoutManager(context)
        adapter = Rows()
        setPadding(PADDING, PADDING, PADDING, PADDING)
        this.clipToPadding = clipToPadding
        setBackgroundColor(screenColor)
        StickySectionHeaders().attachTo(this)
        measure(exactly(WIDTH), exactly(HEIGHT))
        layout(0, 0, WIDTH, HEIGHT)
    }

    private fun render(list: RecyclerView, name: String): Bitmap {
        list.measure(exactly(WIDTH), exactly(HEIGHT))
        list.layout(0, 0, WIDTH, HEIGHT)
        val bitmap = Bitmap.createBitmap(WIDTH, HEIGHT, Bitmap.Config.ARGB_8888)
        list.draw(Canvas(bitmap))
        System.getenv("TEST_UNDECLARED_OUTPUTS_DIR")?.let { dir ->
            File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
        }
        return bitmap
    }

    private fun exactly(size: Int) = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private class Rows : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position % SECTION == 0

        override fun getItemCount(): Int = SECTION * SECTIONS

        override fun getItemViewType(position: Int): Int = if (isSectionHeader(position)) HEADER else ROW

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder =
            object : RecyclerView.ViewHolder(if (viewType == HEADER) header(parent) else card(parent)) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as TextView).text = position.toString()
        }

        private fun header(parent: ViewGroup) = TextView(parent.context).apply {
            layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, HEADER_HEIGHT)
            setTextColor(Color.WHITE)
        }

        private fun card(parent: ViewGroup) = TextView(parent.context).apply {
            layoutParams = ViewGroup.MarginLayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ROW_HEIGHT)
            setBackgroundColor(CARD)
        }
    }

    private companion object {
        const val WIDTH = 600
        const val HEIGHT = 900
        const val PADDING = 40
        const val HEADER_HEIGHT = 80
        const val ROW_HEIGHT = 150
        const val SECTION = 4
        const val SECTIONS = 6
        const val HEADER = 0
        const val ROW = 1
        const val MID_SCROLL = 1_000
        const val PUSH_SCROLL = 2 * (HEADER_HEIGHT + 3 * ROW_HEIGHT) - HEADER_HEIGHT / 2
        const val PNG_QUALITY = 100
        val CARD = Color.rgb(200, 60, 60)
    }
}

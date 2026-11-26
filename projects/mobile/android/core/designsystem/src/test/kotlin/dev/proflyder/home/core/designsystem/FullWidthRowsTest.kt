package dev.proflyder.home.core.designsystem

import android.graphics.Canvas
import android.graphics.Paint
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.designsystem.list.addFullWidthRows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FullWidthRowsTest {
    @Test
    fun noDividerIsDrawnUnderThePinnedSectionHeader() {
        val list = sectionedList()
        list.scrollBy(0, ROW + ROW / 2)
        shadowOf(Looper.getMainLooper()).idle()
        val header = checkNotNull((list.layoutManager as StickyHeadersLayoutManager).stickyHeaderView)
        val pinnedBottom = header.bottom + header.translationY
        val dividers = dividerBottoms(list)
        assertTrue("dividers are drawn", dividers.isNotEmpty())
        assertTrue("divider under the pinned header: $dividers", dividers.all { it > pinnedBottom })
    }

    @Test
    fun dividersStayBetweenRowsWhenNothingIsPinnedOverThem() {
        val dividers = dividerBottoms(sectionedList())
        assertTrue("row dividers: $dividers", dividers.contains(ROW * 2f))
    }

    private fun sectionedList(): RecyclerView {
        val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), R.style.Base_Theme_Home)
        val list = RecyclerView(context)
        list.layoutManager = StickyHeadersLayoutManager(context)
        list.adapter = Rows()
        list.addFullWidthRows()
        list.measure(exactly(WIDTH), exactly(HEIGHT))
        list.layout(0, 0, WIDTH, HEIGHT)
        shadowOf(Looper.getMainLooper()).idle()
        return list
    }

    private fun dividerBottoms(list: RecyclerView): List<Float> {
        val bottoms = mutableListOf<Float>()
        val canvas = object : Canvas() {
            override fun drawRect(left: Float, top: Float, right: Float, bottom: Float, paint: Paint) {
                bottoms += bottom
            }
        }
        (0 until list.itemDecorationCount).forEach { list.getItemDecorationAt(it).onDraw(canvas, list, RecyclerView.State()) }
        return bottoms
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private class Rows : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position % SECTION == 0

        override fun getItemCount(): Int = SECTION * 3

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
            object : RecyclerView.ViewHolder(row(parent)) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) = Unit

        private fun row(parent: ViewGroup): View =
            View(parent.context).apply { layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ROW) }
    }

    private companion object {
        const val WIDTH = 400
        const val HEIGHT = 800
        const val ROW = 100
        const val SECTION = 5
    }
}

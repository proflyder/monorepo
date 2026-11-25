package dev.proflyder.home.core.designsystem.glass

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.R
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class GlassChromeTest {
    private val chrome = GlassChrome()
    private lateinit var activity: Activity
    private lateinit var context: Context

    @Before
    fun setUp() {
        val controller = Robolectric.buildActivity(Activity::class.java)
        controller.get().setTheme(R.style.Base_Theme_Home)
        activity = controller.setup().get()
        context = HostContext(ContextThemeWrapper(activity, R.style.Base_Theme_Home), chrome)
        chrome.updateBar(BAR)
    }

    @Test
    fun listTopPaddingAddsTheBarToTheDesignSpacing() {
        val list = verticalList().apply { setPadding(0, SPACING, 0, SPACING) }
        show(list)
        assertSame(chrome, context.glassChrome())
        assertEquals(BAR + SPACING, list.paddingTop)
        assertEquals(SPACING, list.paddingBottom)
        assertFalse(list.clipToPadding)
    }

    @Test
    fun listFollowsTheBarAndStopsWhenDetached() {
        val list = verticalList()
        show(list)
        chrome.updateBar(BAR * 2)
        assertEquals(BAR * 2, list.paddingTop)
        activity.setContentView(FrameLayout(activity))
        chrome.updateBar(BAR / 2)
        assertEquals(BAR * 2, list.paddingTop)
        show(list, bind = false)
        assertEquals(BAR / 2, list.paddingTop)
    }

    @Test
    fun screenWithoutScrollerPadsItsRoot() {
        val root = FrameLayout(context).apply { addView(TextView(context)) }
        show(root)
        assertEquals(BAR, root.paddingTop)
    }

    @Test
    fun listBelowFixedContentPadsTheScreenInstead() {
        val list = verticalList().apply { setPadding(0, SPACING, 0, 0) }
        val root = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(context))
            addView(list)
        }
        show(root)
        assertEquals(BAR, root.paddingTop)
        assertEquals(SPACING, list.paddingTop)
    }

    @Test
    fun screenHeaderSitsUnderTheBarAndContentStartsBelowIt() {
        val (root, list, header) = screenWithHeader()
        show(root)
        assertEquals(HEADER, chrome.headerHeight)
        assertEquals(BAR.toFloat(), header.translationY)
        assertEquals(BAR + HEADER, list.paddingTop)
        assertEquals(BAR + HEADER, list.getChildAt(0).top)
        assertEquals(0, root.paddingTop)
    }

    @Test
    fun screenHeaderHidesWhileScrollingDownAndReturnsAtTheTop() {
        val (root, list) = screenWithHeader()
        show(root)
        list.scrollBy(0, HEADER / 2)
        settle()
        assertEquals(HEADER / 2, chrome.headerHidden)
        list.scrollBy(0, HEADER * 4)
        settle()
        assertEquals(HEADER, chrome.headerHidden)
        assertEquals(BAR, chrome.glassBottom)
        list.scrollBy(0, -HEADER * 10)
        settle()
        assertEquals(0, chrome.headerHidden)
    }

    @Test
    fun detachedScreenHeaderReleasesTheChrome() {
        val (root) = screenWithHeader()
        show(root)
        activity.setContentView(FrameLayout(activity))
        assertEquals(0, chrome.headerHeight)
        assertEquals(BAR, chrome.contentTop)
    }

    @Test
    fun nestedListsUnderAScreenHeaderKeepTheirPadding() {
        val nested = verticalList()
        val header = GlassScreenHeader(context).apply { addView(nested, ViewGroup.LayoutParams(WIDTH, HEADER)) }
        nested.applyGlassChrome()
        val root = FrameLayout(context).apply {
            addView(verticalList())
            addView(header, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        show(root)
        assertEquals(0, nested.paddingTop)
    }

    @Test
    fun sectionPinsUnderTheGlassWithAGlassBackground() {
        val list = verticalList(StickyHeadersLayoutManager(context)).apply { adapter = Sections() }
        show(list)
        list.scrollBy(0, ROW * 3)
        settle()
        val manager = list.layoutManager as StickyHeadersLayoutManager
        val pinned = checkNotNull(manager.stickyHeaderView)
        assertEquals(BAR, pinned.top)
        assertTrue(pinned.background is PinnedGlassDrawable)
        assertEquals(BAR + ROW, chrome.pinnedSectionBottom)
        assertEquals(BAR + ROW, chrome.edgeBottom)
    }

    @Test
    fun releasedSectionGetsItsOwnBackgroundBack() {
        val list = verticalList(StickyHeadersLayoutManager(context)).apply { adapter = Sections() }
        show(list)
        list.scrollBy(0, ROW * 3)
        settle()
        val pinned = checkNotNull((list.layoutManager as StickyHeadersLayoutManager).stickyHeaderView)
        list.scrollBy(0, -ROW * 10)
        settle()
        assertFalse(pinned.background is PinnedGlassDrawable)
        assertNull(chrome.pinnedSectionBottom)
        assertEquals(BAR, chrome.edgeBottom)
    }

    @Test
    fun sectionPinsUnderAVisibleScreenHeader() {
        val list = verticalList(StickyHeadersLayoutManager(context)).apply { adapter = Sections() }
        val header = GlassScreenHeader(context).apply { addView(View(context), ViewGroup.LayoutParams(WIDTH, HEADER)) }
        val root = FrameLayout(context).apply {
            addView(list)
            addView(header, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        show(root)
        list.scrollBy(0, BAR + HEADER + ROW * 3)
        list.scrollBy(0, -HEADER)
        settle()
        val pinned = checkNotNull((list.layoutManager as StickyHeadersLayoutManager).stickyHeaderView)
        assertEquals(0, chrome.headerHidden)
        assertEquals(BAR + HEADER, pinned.top)
    }

    @Test
    fun glassTintIsFrostedAndTheFallbackIsTranslucent() {
        val style = GlassStyle(context)
        val blurred = style.blurredTintAlpha / OPAQUE
        val fallback = style.fallbackTintAlpha / OPAQUE
        assertTrue("tint over blur is $blurred", blurred in FROSTED_MIN..FROSTED_MAX)
        assertTrue("fallback tint is $fallback", fallback > blurred && fallback < 1f)
        assertTrue(style.blurRadius >= context.resources.displayMetrics.density * MIN_BLUR_DP)
    }

    private fun screenWithHeader(): Triple<FrameLayout, RecyclerView, GlassScreenHeader> {
        val list = verticalList().apply { adapter = Sections() }
        val header = GlassScreenHeader(context).apply { addView(View(context), ViewGroup.LayoutParams(WIDTH, HEADER)) }
        val root = FrameLayout(context).apply {
            addView(list)
            addView(header, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        return Triple(root, list, header)
    }

    private fun verticalList(manager: RecyclerView.LayoutManager = LinearLayoutManager(context)) =
        RecyclerView(context).apply { layoutManager = manager }

    private fun show(view: View, bind: Boolean = true) {
        if (bind) view.applyGlassChrome()
        (view.parent as? ViewGroup)?.removeView(view)
        activity.setContentView(FrameLayout(activity).apply { addView(view, ViewGroup.LayoutParams(WIDTH, HEIGHT)) })
        settle()
        settle()
    }

    private fun settle() {
        activity.window.decorView.requestLayout()
        shadowOf(Looper.getMainLooper()).idle()
    }

    private class Sections : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position % SECTION == 0

        override fun getItemCount(): Int = SECTION * SECTIONS

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view = TextView(parent.context).apply { layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ROW) }
            return object : RecyclerView.ViewHolder(view) {}
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as TextView).text = position.toString()
        }
    }

    private class HostContext(base: Context, override val glassChrome: GlassChrome) : ContextWrapper(base), AppBarHost

    private companion object {
        const val WIDTH = 400
        const val HEIGHT = 800
        const val BAR = 120
        const val HEADER = 80
        const val ROW = 60
        const val SPACING = 8
        const val SECTION = 6
        const val SECTIONS = 10
        const val OPAQUE = 255f
        const val FROSTED_MIN = 0.6f
        const val FROSTED_MAX = 0.75f
        const val MIN_BLUR_DP = 16
    }
}

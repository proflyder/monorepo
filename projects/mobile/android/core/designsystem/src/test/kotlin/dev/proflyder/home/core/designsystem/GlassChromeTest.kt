package dev.proflyder.home.core.designsystem.glass

import android.app.Activity
import android.content.Context
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.R
import dev.proflyder.home.core.designsystem.list.ListHeaderSpacer
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.designsystem.list.sections
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class GlassChromeTest {
    private lateinit var activity: Activity
    private lateinit var context: Context

    @Before
    fun setUp() {
        activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        context = ContextThemeWrapper(activity, R.style.Base_Theme_Home)
    }

    @Test
    fun screenHeaderTakesTheChromeInsetAndContentStartsBelowIt() {
        val content = InsetProbe(context)
        val header = GlassScreenHeader(context).apply { addView(View(context), ViewGroup.LayoutParams(WIDTH, HEADER)) }
        val layout = GlassHeaderLayout(context).apply {
            addView(content)
            addView(header, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        show(layout)
        assertEquals(BAR, header.paddingTop)
        assertEquals(BAR + HEADER, header.height)
        assertEquals(BAR + HEADER, content.insetTop)
        assertEquals(NAV, content.insetBottom)
    }

    @Test
    fun hiddenScreenHeaderPassesTheChromeInsetThrough() {
        val content = InsetProbe(context)
        val header = GlassScreenHeader(context).apply { visibility = View.GONE }
        val layout = GlassHeaderLayout(context).apply {
            addView(content)
            addView(header)
        }
        show(layout)
        assertEquals(BAR, content.insetTop)
    }

    @Test
    fun headerContentDoesNotGetTheChromeInsetAgain() {
        val nested = InsetProbe(context)
        val header = GlassScreenHeader(context).apply { addView(nested) }
        show(GlassHeaderLayout(context).apply { addView(header) })
        assertTrue("nested got ${nested.insetTop}", nested.insetTop <= 0)
    }

    @Test
    fun chromeGlassExtendsOverAttachedBlocksAndDrawsThemItself() {
        val chrome = GlassChromeLayout(context)
        val block = View(context)
        val root = FrameLayout(context).apply {
            addView(chrome, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, BAR))
            addView(block, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, HEADER).apply { topMargin = BAR })
        }
        show(root)
        assertEquals(BAR, chrome.glassBottom)
        chrome.attachBlock(block)
        assertEquals(0f, block.alpha)
        assertEquals(BAR + HEADER, chrome.glassBottom)
        chrome.detachBlock(block)
        assertEquals(1f, block.alpha)
        assertEquals(BAR, chrome.glassBottom)
    }

    @Test
    fun listHeaderFollowsItsSpacerAndScrollsAway() {
        val list = RecyclerView(context).apply { layoutManager = StickyHeadersLayoutManager(context) }
        val header = View(context)
        list.adapter = ConcatAdapter(ListHeaderSpacer.leading(list, header), Rows())
        val root = FrameLayout(context).apply {
            addView(list)
            addView(header, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, HEADER))
        }
        list.setPadding(0, BAR, 0, 0)
        list.clipToPadding = false
        show(root)
        assertEquals(BAR.toFloat(), header.translationY)
        assertEquals(BAR + HEADER, list.getChildAt(1).top)
        list.scrollBy(0, HEADER / 2)
        settle()
        assertEquals((BAR - HEADER / 2).toFloat(), header.translationY)
        list.scrollBy(0, HEIGHT)
        settle()
        assertTrue(header.translationY + HEADER <= 0f)
    }

    @Test
    fun concatenatedAdaptersExposeTheirSections() {
        val sections = checkNotNull(ConcatAdapter(Rows(), Footer()).sections())
        assertTrue(sections.isStickyHeader(0))
        assertFalse(sections.isStickyHeader(1))
        assertTrue(sections.isStickyHeader(SECTION))
        assertFalse("footer never pins", sections.isStickyHeader(SECTION * 2))
        assertTrue("footer is outside every group", sections.isSectionHeader(SECTION * 2))
    }

    @Test
    fun glassTintIsFrostedAndTheFallbackIsTranslucent() {
        val style = GlassStyle(context)
        val blurred = style.blurredTintAlpha / OPAQUE
        val fallback = style.fallbackTintAlpha / OPAQUE
        assertTrue("tint over blur is $blurred", blurred in FROSTED_MIN..FROSTED_MAX)
        assertTrue("fallback tint is $fallback", fallback > blurred && fallback < 1f)
    }

    private fun show(view: View) {
        activity.setContentView(FrameLayout(activity).apply { addView(view, ViewGroup.LayoutParams(WIDTH, HEIGHT)) })
        settle()
        repeat(2) {
            ViewCompat.dispatchApplyWindowInsets(view, insets())
            settle()
        }
    }

    private fun insets(): WindowInsetsCompat = WindowInsetsCompat.Builder()
        .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, BAR, 0, 0))
        .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, NAV))
        .build()

    private fun settle() {
        activity.window.decorView.requestLayout()
        shadowOf(Looper.getMainLooper()).idle()
    }

    private class InsetProbe(context: Context) : View(context) {
        var insetTop = -1
        var insetBottom = -1

        init {
            ViewCompat.setOnApplyWindowInsetsListener(this) { _, insets -> record(insets) }
        }

        private fun record(insets: WindowInsetsCompat): WindowInsetsCompat {
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            insetTop = bars.top
            insetBottom = bars.bottom
            return insets
        }
    }

    private class Rows : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position % SECTION == 0

        override fun getItemCount(): Int = SECTION * 2

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
            object : RecyclerView.ViewHolder(View(parent.context).apply { layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ROW) }) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) = Unit
    }

    private class Footer : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun getItemCount(): Int = 1

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) = object : RecyclerView.ViewHolder(View(parent.context)) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) = Unit
    }

    private companion object {
        const val WIDTH = 400
        const val HEIGHT = 800
        const val BAR = 120
        const val NAV = 90
        const val HEADER = 80
        const val SECTION = 4
        const val ROW = 100
        const val OPAQUE = 255f
        const val FROSTED_MIN = 0.6f
        const val FROSTED_MAX = 0.75f
    }
}

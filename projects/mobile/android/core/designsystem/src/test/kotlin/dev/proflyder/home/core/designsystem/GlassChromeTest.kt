package dev.proflyder.home.core.designsystem.glass

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.view.ContextThemeWrapper
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.AppBarLayout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class GlassChromeTest {
    @Test
    fun listTopPaddingEqualsTheBarHeight() {
        val source = AppBarHeightTracker().apply { update(120) }
        val context = SourceContext(RuntimeEnvironment.getApplication(), source)
        val list = RecyclerView(context).apply {
            layoutManager = LinearLayoutManager(context)
            setPadding(0, 0, 0, 12)
        }
        list.applyAppBarContentPadding()
        assertEquals(source.appBarHeight, list.paddingTop)
        assertEquals(12, list.paddingBottom)
    }

    @Test
    fun listFollowsTheBarHeightChanges() {
        val source = AppBarHeightTracker().apply { update(80) }
        val context = SourceContext(RuntimeEnvironment.getApplication(), source)
        val list = RecyclerView(context).apply {
            layoutManager = LinearLayoutManager(context)
            setPadding(0, 4, 0, 0)
        }
        list.applyAppBarContentPadding()
        assertEquals(84, list.paddingTop)
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        activity.setContentView(list)
        source.update(184)
        assertEquals(188, list.paddingTop)
        activity.setContentView(FrameLayout(activity))
        source.update(60)
        assertEquals(188, list.paddingTop)
        activity.setContentView(list)
        assertEquals(64, list.paddingTop)
    }

    @Test
    fun screenWithoutScrollerPadsItsRoot() {
        val source = AppBarHeightTracker().apply { update(96) }
        val context = SourceContext(RuntimeEnvironment.getApplication(), source)
        val root = FrameLayout(context).apply { addView(TextView(context)) }
        root.applyAppBarContentPadding()
        assertEquals(96, root.paddingTop)
    }

    @Test
    fun listUnderAnInnerAppBarPadsThatAppBar() {
        val source = AppBarHeightTracker().apply { update(100) }
        val themed = ContextThemeWrapper(RuntimeEnvironment.getApplication(), MaterialR.style.Theme_Material3_DayNight_NoActionBar)
        val context = SourceContext(themed, source)
        val inner = AppBarLayout(context)
        val list = RecyclerView(context).apply { layoutManager = LinearLayoutManager(context) }
        val root = FrameLayout(context).apply {
            addView(inner)
            addView(list)
        }
        root.applyAppBarContentPadding()
        assertEquals(100, inner.paddingTop)
        assertEquals(0, list.paddingTop)
        assertEquals(false, list.clipToPadding)
    }

    @Test
    fun listBelowFixedContentPadsTheScreenInstead() {
        val source = AppBarHeightTracker().apply { update(90) }
        val context = SourceContext(RuntimeEnvironment.getApplication(), source)
        val list = RecyclerView(context).apply {
            layoutManager = LinearLayoutManager(context)
            setPadding(0, 6, 0, 0)
        }
        val root = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(context))
            addView(list)
        }
        root.applyAppBarContentPadding()
        assertEquals(90, root.paddingTop)
        assertEquals(6, list.paddingTop)
    }

    @Test
    fun innerAppBarScrolledAwayIsNotDrawnUnderTheBar() {
        val source = AppBarHeightTracker().apply { update(100) }
        val themed = ContextThemeWrapper(RuntimeEnvironment.getApplication(), MaterialR.style.Theme_Material3_DayNight_NoActionBar)
        val context = SourceContext(themed, source)
        val inner = AppBarLayout(context)
        val root = FrameLayout(context).apply {
            addView(inner)
            addView(RecyclerView(context).apply { layoutManager = LinearLayoutManager(context) })
        }
        root.applyAppBarContentPadding()
        inner.layout(0, 0, 400, 180)
        HideUnderBar.onOffsetChanged(inner, -80)
        assertEquals(Rect(0, 180, 400, 180), inner.clipBounds)
        HideUnderBar.onOffsetChanged(inner, 0)
        assertEquals(Rect(0, 100, 400, 180), inner.clipBounds)
    }

    @Test
    fun blurredGlassKeepsTheContentVisible() {
        val bar = GlassBarView(themedContext())
        val alpha = bar.blurredPainter.tintAlpha / OPAQUE
        assertTrue("tint over blur is $alpha", alpha in FROSTED_MIN..FROSTED_MAX)
        assertTrue(bar.blurRadius >= bar.resources.displayMetrics.density * MIN_BLUR_DP)
    }

    @Test
    fun pinnedHeaderExtendsTheGlassSurface() {
        val context = themedContext()
        val bar = GlassBarView(context)
        bar.setBarHeight(120, 40)
        val header = TextView(context).apply { layout(0, 0, 400, 60) }
        bar.setPinnedHeader(PinnedBarHeader(header, 0, -20))
        assertEquals(160, bar.surfaceBottom)
        bar.setPinnedHeader(null)
        assertEquals(120, bar.surfaceBottom)
    }

    @Test
    fun scrolledContentInvalidatesTheBlurSnapshot() {
        val bar = GlassBarView(themedContext())
        val tracker = AppBarHeightTracker().apply { backdropListener = bar::requestSnapshot }
        bar.draw(Canvas(Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)))
        tracker.backdropChanged()
        assertTrue(bar.snapshotDirty)
    }

    private fun themedContext() = ContextThemeWrapper(RuntimeEnvironment.getApplication(), MaterialR.style.Theme_Material3_DayNight_NoActionBar)

    private companion object {
        const val OPAQUE = 255f
        const val FROSTED_MIN = 0.6f
        const val FROSTED_MAX = 0.75f
        const val MIN_BLUR_DP = 16
    }

    private class SourceContext(
        base: Context,
        private val source: AppBarHost,
    ) : ContextWrapper(base), AppBarHost by source
}

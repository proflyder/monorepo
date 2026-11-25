package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import android.view.ContextThemeWrapper
import android.widget.FrameLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.AppBarLayout
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
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
        source.update(184)
        assertEquals(188, list.paddingTop)
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

    private class SourceContext(
        base: Context,
        private val source: AppBarHost,
    ) : ContextWrapper(base), AppBarHost by source
}

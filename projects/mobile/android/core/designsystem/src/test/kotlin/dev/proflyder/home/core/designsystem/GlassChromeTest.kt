package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class GlassChromeTest {
    @Test
    fun listTopPaddingEqualsTheBarHeight() {
        val source = FakeSource(barHeight = 120)
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
        val source = FakeSource(barHeight = 80)
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

    private class FakeSource(barHeight: Int) : AppBarHeightSource {
        private val listeners = mutableListOf<AppBarHeightListener>()
        private var height = barHeight

        override val appBarHeight: Int get() = height

        override fun addAppBarHeightListener(listener: AppBarHeightListener) {
            listeners.add(listener)
            listener.onAppBarHeightChanged(height)
        }

        override fun removeAppBarHeightListener(listener: AppBarHeightListener) {
            listeners.remove(listener)
        }

        fun update(next: Int) {
            height = next
            listeners.toList().forEach { it.onAppBarHeightChanged(next) }
        }
    }

    private class SourceContext(
        base: Context,
        private val source: AppBarHeightSource,
    ) : ContextWrapper(base), AppBarHeightSource by source
}

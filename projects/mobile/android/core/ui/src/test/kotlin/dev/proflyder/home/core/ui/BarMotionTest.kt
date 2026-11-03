package dev.proflyder.home.core.ui

import android.view.View
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * The bar motion must always end in a self-consistent state, including when a
 * running animation is interrupted or animations are disabled.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BarMotionTest {
    private fun view() = View(RuntimeEnvironment.getApplication())

    @Test
    fun snapEnterThenSnapExitEndsHidden() {
        val view = view()

        view.animateBar(visible = true, hiddenTranslationY = -120f, animate = false)
        assertEquals(View.VISIBLE, view.visibility)
        assertEquals(0f, view.translationY, 0f)
        assertEquals(1f, view.alpha, 0f)

        view.animateBar(visible = false, hiddenTranslationY = -120f, animate = false)
        assertEquals(View.GONE, view.visibility)
        assertEquals(-120f, view.translationY, 0f)
        assertEquals(0f, view.alpha, 0f)
    }

    @Test
    fun interruptedAnimationStillEndsHidden() {
        val view = view()

        // Start entering, then interrupt with an exit while it is still running
        // and animations are disabled: the result must be the exit end state.
        view.animateBar(visible = true, hiddenTranslationY = 200f, animate = true)
        view.animateBar(visible = false, hiddenTranslationY = 200f, animate = false)

        assertEquals(View.GONE, view.visibility)
        assertEquals(200f, view.translationY, 0f)
        assertEquals(0f, view.alpha, 0f)
    }

    @Test
    fun showAfterSnapExitStartsFromHiddenEdge() {
        val view = view()
        view.animateBar(visible = false, hiddenTranslationY = 200f, animate = false)

        view.animateBar(visible = true, hiddenTranslationY = 200f, animate = false)
        assertEquals(View.VISIBLE, view.visibility)
        assertEquals(0f, view.translationY, 0f)
        assertEquals(1f, view.alpha, 0f)
    }

    @Test
    fun paddingSnapUsesTargetValues() {
        val view = view()

        view.animateVerticalPadding(topPx = 60, bottomPx = 96, entering = true, animate = false)
        assertEquals(60, view.paddingTop)
        assertEquals(96, view.paddingBottom)

        view.animateVerticalPadding(topPx = 12, bottomPx = 96, entering = false, animate = false)
        assertEquals(12, view.paddingTop)
        assertEquals(96, view.paddingBottom)
    }
}

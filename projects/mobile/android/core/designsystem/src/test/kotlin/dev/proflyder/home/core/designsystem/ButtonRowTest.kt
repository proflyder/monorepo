package dev.proflyder.home.core.designsystem

import android.view.LayoutInflater
import android.view.View
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.theme.AppTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w360dp-h780dp-night-xxhdpi")
class ButtonRowTest {
    @Test
    fun loneSecondaryActionBecomesFilled() = AppTheme.entries.forEach(::assertLone)

    private fun assertLone(theme: AppTheme) {
        val (state, secondary, main) = emptyState(theme)
        main.visibility = View.GONE
        layout(state)
        assertEquals("$theme lone action", primary(state), secondary.backgroundTintList?.defaultColor)
    }

    @Test
    fun pairHasOneFilledMainAction() = AppTheme.entries.forEach(::assertPair)

    private fun assertPair(theme: AppTheme) {
        val (state, secondary, main) = emptyState(theme)
        layout(state)
        assertEquals("$theme main", primary(state), main.backgroundTintList?.defaultColor)
        val tonal = color(state.context, MaterialR.attr.colorSecondaryContainer)
        assertEquals("$theme secondary", tonal, secondary.backgroundTintList?.defaultColor)
        assertEquals("$theme equal width", main.width, secondary.width)
    }

    @Test
    fun pairReturnsToTonalWhenTheMainActionComesBack() {
        val (state, secondary, main) = emptyState(AppTheme.NEON)
        main.visibility = View.GONE
        layout(state)
        main.visibility = View.VISIBLE
        layout(state)
        assertTrue(secondary.backgroundTintList?.defaultColor != primary(state))
    }

    private fun emptyState(theme: AppTheme): Triple<EmptyState, MaterialButton, MaterialButton> {
        val state = LayoutInflater.from(themed(theme)).inflate(R.layout.ds_catalog, null, false)
            .let { DesignCatalog.bind(it); it }
            .findViewById<View>(R.id.ds_catalog_states)
            .let { findEmptyState(it) }
        return Triple(state, state.secondaryAction, state.action)
    }

    private fun findEmptyState(view: View): EmptyState = when (view) {
        is EmptyState -> view
        else -> (0 until (view as android.view.ViewGroup).childCount).map(view::getChildAt).first { it is EmptyState } as EmptyState
    }

    private fun primary(view: View): Int = color(view.context, MaterialR.attr.colorPrimary)

    private fun layout(view: View) {
        val width = (WIDTH_DP * view.resources.displayMetrics.density).toInt()
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
    }

    private companion object {
        const val WIDTH_DP = 360
    }
}

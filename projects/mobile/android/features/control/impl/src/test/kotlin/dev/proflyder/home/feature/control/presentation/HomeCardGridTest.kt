package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceGroupBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubBinding
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import dev.proflyder.home.core.designsystem.R as DesignR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
class HomeCardGridTest {
    private val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), DesignR.style.Base_Theme_Home)
        .apply { theme.applyStyle(AppTheme.EMERALD.overlayRes, true) }
    private val inflater = LayoutInflater.from(context)
    private val parent = FrameLayout(context)

    @Test
    fun alarmCardsAndTheHubShareTheDeviceCardEdgesAndGaps() {
        val home = FragmentControlBinding.inflate(inflater)
        val device = margins(ItemControlDeviceGroupBinding.inflate(inflater, parent, false).root.layoutParams)
        val hub = ItemControlHubBinding.inflate(inflater, parent, false).root
        val cardEdge = home.controlList.paddingStart + device.leftMargin

        assertEquals(cardEdge, home.controlAlarmsSlot.paddingStart)
        assertEquals(cardEdge, home.controlAlarmsSlot.paddingEnd)
        assertEquals(device.leftMargin, margins(hub.layoutParams).leftMargin)
        assertEquals(device.topMargin + device.bottomMargin, hub.dividerDrawable.intrinsicHeight)
    }

    private fun margins(params: ViewGroup.LayoutParams): ViewGroup.MarginLayoutParams = params as ViewGroup.MarginLayoutParams
}

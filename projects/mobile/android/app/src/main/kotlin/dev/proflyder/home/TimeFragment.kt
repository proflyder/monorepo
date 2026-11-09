package dev.proflyder.home

import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.viewpager2.adapter.FragmentStateAdapter
import androidx.viewpager2.widget.ViewPager2
import com.google.android.material.tabs.TabLayoutMediator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.databinding.FragmentTimeBinding

/**
 * Container for the Alarms / Timers / Reminders screens. The sections live in a
 * [ViewPager2], so a horizontal swipe and the pill tabs drive the same selected
 * page. The selected section is restored from the Fragment's saved state on
 * rotation and from route arguments on a deep link.
 */
class TimeFragment : BaseFragment<FragmentTimeBinding>(FragmentTimeBinding.Companion::inflate) {
    internal var section: TimeSection = TimeSection.ALARMS
        private set

    private var tabsMediator: TabLayoutMediator? = null

    private val pageCallback = object : ViewPager2.OnPageChangeCallback() {
        override fun onPageSelected(position: Int) = onPageChanged(position)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        section = savedSection(savedInstanceState)
            ?: TimeSection.fromRouteSegment(arguments?.getString(TimeDestination.ARG_SECTION))
            ?: TimeSection.ALARMS
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString(KEY_SECTION, section.routeSegment)
        super.onSaveInstanceState(outState)
    }

    override fun onBindingCreated(binding: FragmentTimeBinding, savedInstanceState: Bundle?) {
        binding.timePager.adapter = TimePagerAdapter(this)
        binding.timePager.registerOnPageChangeCallback(pageCallback)
        tabsMediator = TabLayoutMediator(binding.timeTabs, binding.timePager) { tab, position ->
            tab.setCustomView(R.layout.view_time_tab)
            tab.text = getString(titleResFor(TimeSection.entries[position]))
        }.apply { attach() }
        binding.timePager.setCurrentItem(section.ordinal, false)
    }

    override fun onBindingDestroyed() {
        tabsMediator?.detach()
        tabsMediator = null
        binding.timePager.unregisterOnPageChangeCallback(pageCallback)
        binding.timePager.adapter = null
    }

    private fun savedSection(state: Bundle?): TimeSection? =
        TimeSection.fromRouteSegment(state?.getString(KEY_SECTION))

    internal fun titleResFor(section: TimeSection): Int = when (section) {
        TimeSection.ALARMS -> R.string.app_time_alarms
        TimeSection.TIMERS -> R.string.app_time_timers
        TimeSection.REMINDERS -> R.string.app_time_reminders
    }

    /** Swipe or tab tap landed on another page: both controls already follow it. */
    internal fun onPageChanged(position: Int) {
        section = TimeSection.entries.getOrNull(position) ?: return
    }

    private companion object {
        const val KEY_SECTION = "time_section"
    }
}

/** One page per Time section; stable ids let the pager restore its state. */
internal class TimePagerAdapter(host: Fragment) : FragmentStateAdapter(host) {
    override fun getItemCount(): Int = TimeSection.entries.size

    override fun getItemId(position: Int): Long = TimeSection.entries[position].ordinal.toLong()

    override fun containsItem(itemId: Long): Boolean =
        TimeSection.entries.any { it.ordinal.toLong() == itemId }

    override fun createFragment(position: Int): Fragment =
        sectionFragment(TimeSection.entries[position])
}

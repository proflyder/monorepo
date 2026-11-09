package dev.proflyder.home

import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.viewpager2.adapter.FragmentStateAdapter
import androidx.viewpager2.widget.ViewPager2
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.databinding.FragmentTimeBinding

/**
 * Container for the Alarms / Timers / Reminders screens. The sections live in a
 * [ViewPager2], so a horizontal swipe and the segmented switch drive the same
 * selected page. The selected section is restored from the Fragment's saved
 * state on rotation and from route arguments on a deep link.
 */
class TimeFragment : BaseFragment<FragmentTimeBinding>(FragmentTimeBinding.Companion::inflate) {
    internal var section: TimeSection = TimeSection.ALARMS
        private set

    private var syncingToggle = false

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
        binding.timeTabs.addOnButtonCheckedListener { _, checkedId, isChecked ->
            if (syncingToggle || !isChecked) return@addOnButtonCheckedListener
            select(currentSectionFor(checkedId))
        }
        syncToggle()
        binding.timePager.setCurrentItem(section.ordinal, false)
    }

    override fun onBindingDestroyed() {
        binding.timePager.unregisterOnPageChangeCallback(pageCallback)
        binding.timePager.adapter = null
    }

    private fun savedSection(state: Bundle?): TimeSection? =
        TimeSection.fromRouteSegment(state?.getString(KEY_SECTION))

    internal fun currentSectionFor(checkedId: Int): TimeSection = when (checkedId) {
        R.id.time_tab_timers -> TimeSection.TIMERS
        R.id.time_tab_reminders -> TimeSection.REMINDERS
        else -> TimeSection.ALARMS
    }

    internal fun buttonIdFor(section: TimeSection): Int = when (section) {
        TimeSection.ALARMS -> R.id.time_tab_alarms
        TimeSection.TIMERS -> R.id.time_tab_timers
        TimeSection.REMINDERS -> R.id.time_tab_reminders
    }

    internal fun syncToggle() {
        syncingToggle = true
        binding.timeTabs.check(buttonIdFor(section))
        syncingToggle = false
    }

    /** Swipe landed on another page: mirror it in the segmented switch. */
    internal fun onPageChanged(position: Int) {
        val next = TimeSection.entries.getOrNull(position) ?: return
        section = next
        syncToggle()
    }

    /** A pill was tapped: move the pager, which keeps both controls in sync. */
    internal fun select(next: TimeSection) {
        if (next == section) return
        section = next
        syncToggle()
        binding.timePager.setCurrentItem(next.ordinal, true)
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

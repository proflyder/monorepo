package dev.proflyder.home

import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.viewpager2.adapter.FragmentStateAdapter
import androidx.viewpager2.widget.ViewPager2
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.databinding.FragmentTimeBinding

class TimeFragment : BaseFragment<FragmentTimeBinding>(FragmentTimeBinding.Companion::inflate) {
    internal var section: TimeSection = TimeSection.ALARMS
        private set

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
        binding.timeTabs.setLabels(TimeSection.entries.map { getString(titleResFor(it)) })
        binding.timeTabs.onSelected = { binding.timePager.currentItem = it }
        binding.timePager.setCurrentItem(section.ordinal, false)
        binding.timeTabs.select(section.ordinal)
    }

    override fun onBindingDestroyed() {
        binding.timeTabs.onSelected = null
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

    internal fun onPageChanged(position: Int) {
        section = TimeSection.entries.getOrNull(position) ?: return
        binding.timeTabs.select(position)
    }

    private companion object {
        const val KEY_SECTION = "time_section"
    }
}

internal class TimePagerAdapter(host: Fragment) : FragmentStateAdapter(host) {
    override fun getItemCount(): Int = TimeSection.entries.size

    override fun getItemId(position: Int): Long = TimeSection.entries[position].ordinal.toLong()

    override fun containsItem(itemId: Long): Boolean =
        TimeSection.entries.any { it.ordinal.toLong() == itemId }

    private val features = (host.requireActivity().application as HomeApplication).graph.featureRegistry

    override fun createFragment(position: Int): Fragment {
        val entry = checkNotNull(features.entry(TimeSection.entries[position].routeSegment))
        return entry.createFragment(entry.rootDestination)
    }
}

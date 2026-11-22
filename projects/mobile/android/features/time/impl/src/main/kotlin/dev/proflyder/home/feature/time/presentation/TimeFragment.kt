package dev.proflyder.home.feature.time.presentation

import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.viewpager2.adapter.FragmentStateAdapter
import androidx.viewpager2.widget.ViewPager2
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.runtime.appGraph
import dev.proflyder.home.feature.time.TimeGraph
import dev.proflyder.home.feature.time.TimeSection
import dev.proflyder.home.feature.time.impl.R
import dev.proflyder.home.feature.time.impl.databinding.FragmentTimeBinding

internal class TimeFragment :
    ScreenFragment<FragmentTimeBinding, TimeUiState, TimeIntent, Nothing>(FragmentTimeBinding.Companion::inflate) {
    override val viewModel: TimeViewModel by screenViewModel()

    private val pageCallback = object : ViewPager2.OnPageChangeCallback() {
        override fun onPageSelected(position: Int) = select(position)
    }

    override fun onScreenCreated(binding: FragmentTimeBinding, savedInstanceState: Bundle?) {
        binding.timePager.adapter = TimePagerAdapter(this, requireContext().appGraph<TimeGraph>().featureRegistry)
        binding.timePager.setCurrentItem(viewModel.state.value.section.ordinal, false)
        binding.timePager.registerOnPageChangeCallback(pageCallback)
        binding.timeTabs.setLabels(TimeSection.entries.map { getString(it.titleRes) })
        binding.timeTabs.onSelected = ::select
    }

    override fun render(state: TimeUiState) {
        val index = state.section.ordinal
        if (binding.timePager.currentItem != index) binding.timePager.currentItem = index
        if (binding.timeTabs.selectedIndex != index) binding.timeTabs.select(index)
    }

    override fun onBindingDestroyed() {
        binding.timeTabs.onSelected = null
        binding.timePager.unregisterOnPageChangeCallback(pageCallback)
        binding.timePager.adapter = null
    }

    internal fun select(position: Int) {
        TimeSection.entries.getOrNull(position)?.let { dispatch(TimeIntent.Select(it)) }
    }
}

internal val TimeSection.titleRes: Int
    get() = when (this) {
        TimeSection.ALARMS -> R.string.time_alarms
        TimeSection.TIMERS -> R.string.time_timers
        TimeSection.REMINDERS -> R.string.time_reminders
    }

internal class TimePagerAdapter(host: Fragment, private val features: FeatureRegistry) : FragmentStateAdapter(host) {
    override fun getItemCount(): Int = TimeSection.entries.size

    override fun getItemId(position: Int): Long = TimeSection.entries[position].ordinal.toLong()

    override fun containsItem(itemId: Long): Boolean = TimeSection.entries.any { it.ordinal.toLong() == itemId }

    override fun createFragment(position: Int): Fragment {
        val entry = checkNotNull(features.entry(TimeSection.entries[position].featureId))
        return entry.createFragment(entry.rootDestination)
    }
}

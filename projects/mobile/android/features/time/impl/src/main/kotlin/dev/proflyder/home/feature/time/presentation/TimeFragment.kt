package dev.proflyder.home.feature.time.presentation

import dev.proflyder.home.core.presentation.bindPills
import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.viewpager2.adapter.FragmentStateAdapter
import androidx.viewpager2.widget.ViewPager2
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.presentation.PillSegmentsBinder
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.runtime.appGraph
import dev.proflyder.home.feature.time.TimeGraph
import dev.proflyder.home.feature.time.TimeSection
import dev.proflyder.home.feature.time.impl.databinding.FragmentTimeBinding

internal class TimeFragment :
    ScreenFragment<FragmentTimeBinding, TimeUiState, TimeIntent, Nothing>(FragmentTimeBinding.Companion::inflate) {
    override val viewModel: TimeViewModel by screenViewModel()

    private val pageCallback = object : ViewPager2.OnPageChangeCallback() {
        override fun onPageSelected(position: Int) = select(position)
    }

    override fun onViewReady(binding: FragmentTimeBinding, savedInstanceState: Bundle?) {
        val features = requireContext().appGraph<TimeGraph>().featureRegistry
        binding.timePager.adapter = TimePagerAdapter(this, features)
        binding.timePager.registerOnPageChangeCallback(pageCallback)
        binding.timeTabs.setLabels(TimeSection.entries.map { getString(features.sectionEntry(it).titleRes) })
        val tabs = bindPills(binding.timeTabs, TimeSection.entries, TimeIntent::Select)
        render(TimeUiState::section) { section -> renderSection(tabs, section) }
    }

    private fun renderSection(tabs: PillSegmentsBinder<TimeSection>, section: TimeSection) {
        val index = section.ordinal
        if (binding.timePager.currentItem != index) binding.timePager.setCurrentItem(index, isResumed)
        tabs.render(section)
    }

    override fun onBindingDestroyed() {
        binding.timeTabs.onSelectionRequested = null
        binding.timePager.unregisterOnPageChangeCallback(pageCallback)
        binding.timePager.adapter = null
    }

    internal fun select(position: Int) {
        TimeSection.entries.getOrNull(position)?.let { dispatch(TimeIntent.Select(it)) }
    }
}

internal fun FeatureRegistry.sectionEntry(section: TimeSection): FeatureEntry = checkNotNull(entry(section.featureId))

internal class TimePagerAdapter(host: Fragment, private val features: FeatureRegistry) : FragmentStateAdapter(host) {
    override fun getItemCount(): Int = TimeSection.entries.size

    override fun getItemId(position: Int): Long = TimeSection.entries[position].ordinal.toLong()

    override fun containsItem(itemId: Long): Boolean = TimeSection.entries.any { it.ordinal.toLong() == itemId }

    override fun createFragment(position: Int): Fragment {
        val entry = features.sectionEntry(TimeSection.entries[position])
        return entry.createFragment(entry.rootDestination)
    }
}

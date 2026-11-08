package dev.proflyder.home

import android.os.Bundle
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.databinding.FragmentTimeBinding

/**
 * Container for the Alarms / Timers / Reminders screens. The child Fragment is
 * replaced by the shared FragmentManager, so each existing feature keeps its
 * own ViewModel and behaviour unchanged. The selected section is restored from
 * the Fragment's saved state on rotation and from route arguments on a deep link.
 */
class TimeFragment : BaseFragment<FragmentTimeBinding>(FragmentTimeBinding.Companion::inflate) {
    internal var section: TimeSection = TimeSection.ALARMS
        private set

    private var syncingToggle = false
    private var renderedSection: TimeSection? = null

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
        binding.timeTabs.addOnButtonCheckedListener { _, checkedId, isChecked ->
            if (syncingToggle || !isChecked) return@addOnButtonCheckedListener
            show(currentSectionFor(checkedId))
        }
        syncToggle()
        val restored = savedInstanceState != null &&
            childFragmentManager.findFragmentById(R.id.time_content) != null
        if (!restored) show(section, force = true)
        renderedSection = section
    }

    private fun savedSection(state: Bundle?): TimeSection? =
        TimeSection.fromRouteSegment(state?.getString(KEY_SECTION))

    private fun currentSectionFor(checkedId: Int): TimeSection = when (checkedId) {
        R.id.time_tab_timers -> TimeSection.TIMERS
        R.id.time_tab_reminders -> TimeSection.REMINDERS
        else -> TimeSection.ALARMS
    }

    private fun buttonIdFor(section: TimeSection): Int = when (section) {
        TimeSection.ALARMS -> R.id.time_tab_alarms
        TimeSection.TIMERS -> R.id.time_tab_timers
        TimeSection.REMINDERS -> R.id.time_tab_reminders
    }

    private fun syncToggle() {
        syncingToggle = true
        binding.timeTabs.check(buttonIdFor(section))
        syncingToggle = false
    }

    private fun show(next: TimeSection, force: Boolean = false) {
        section = next
        if (!force && renderedSection == next) return
        renderedSection = next
        syncToggle()
        val existing = childFragmentManager.findFragmentById(R.id.time_content)
        if (existing != null && existing.tag == next.routeSegment) return
        childFragmentManager.beginTransaction()
            .setReorderingAllowed(true)
            .replace(R.id.time_content, sectionFragment(next), next.routeSegment)
            .commit()
    }

    private companion object {
        const val KEY_SECTION = "time_section"
    }
}

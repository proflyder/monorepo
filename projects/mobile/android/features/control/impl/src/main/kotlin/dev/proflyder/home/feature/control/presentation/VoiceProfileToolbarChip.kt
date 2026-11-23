package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.isVisible
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.navigation.AppChromeState
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarContribution
import dev.proflyder.home.core.navigation.ToolbarSlots
import dev.proflyder.home.feature.control.ControlListDestination
import dev.proflyder.home.feature.control.VoiceProfileState
import dev.proflyder.home.feature.control.VoiceProfileSwitch
import dev.proflyder.home.feature.control.di.ControlFeatureScope
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.core.presentation.LeaseRelease
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class, binding = binding<ToolbarContribution>())
@Inject
class VoiceProfileToolbarChip internal constructor(
    private val scope: ControlFeatureScope,
) : ToolbarContribution {
    override val order: Int = 0

    override fun attach(activity: AppCompatActivity, slots: ToolbarSlots, navigator: Navigator) {
        val chip = LayoutInflater.from(slots.start.context)
            .inflate(R.layout.control_profile_chip, slots.start, false) as MaterialButton
        slots.start.addView(chip)
        val lease = scope.acquire()
        activity.lifecycle.addObserver(LeaseRelease(lease))
        val profiles = lease.graph.voiceProfile
        chip.setOnClickListener { activity.lifecycleScope.launch { switch(profiles) } }
        activity.lifecycleScope.launch {
            activity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                combine(profiles.profile, navigator.chrome, ::visibleProfile).collect { render(chip, it) }
            }
        }
    }

    private suspend fun switch(profiles: VoiceProfileSwitch) {
        profiles.profile.first()?.let { profiles.setWork(!it.work) }
    }

    private fun visibleProfile(profile: VoiceProfileState?, chrome: AppChromeState): VoiceProfileState? =
        profile.takeIf { chrome.tabId == ControlListDestination.featureId && !chrome.overlay }

    private fun render(chip: MaterialButton, profile: VoiceProfileState?) {
        chip.isVisible = profile != null
        profile ?: return
        chip.setText(if (profile.work) R.string.control_profile_work else R.string.control_profile_personal)
        chip.contentDescription = chip.context.getString(R.string.control_profile_chip, chip.text)
        chip.isEnabled = profile.available
    }
}

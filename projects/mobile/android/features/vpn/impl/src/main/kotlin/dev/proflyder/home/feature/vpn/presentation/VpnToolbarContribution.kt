package dev.proflyder.home.feature.vpn.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.isVisible
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarContribution
import dev.proflyder.home.core.navigation.ToolbarSlots
import dev.proflyder.home.feature.vpn.VpnDestination
import dev.proflyder.home.feature.vpn.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class, binding = binding<ToolbarContribution>())
@Inject
class VpnToolbarContribution(
    private val hosts: VpnActivityHost.Factory,
) : ToolbarContribution {
    override val order: Int = 10

    override fun attach(activity: AppCompatActivity, slots: ToolbarSlots, navigator: Navigator) {
        val toggle = slots.end.inflateChild(R.layout.vpn_toolbar_toggle) as MaterialButton
        val banner = slots.below.inflateChild(R.layout.vpn_route_banner)
        val host = hosts.create(activity) { navigator.navigate(VpnDestination) }
        host.bind(toggle)
        activity.lifecycleScope.launch {
            activity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                host.observe(this, toggle, banner)
                navigator.chrome.collect { toggle.isVisible = !it.overlay }
            }
        }
    }

    private fun ViewGroup.inflateChild(layout: Int): View =
        LayoutInflater.from(context).inflate(layout, this, false).also(::addView)
}

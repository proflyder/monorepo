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
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.designsystem.RevealBinder
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarContribution
import dev.proflyder.home.core.navigation.ToolbarSlots
import dev.proflyder.home.core.ui.injectedViewModels
import dev.proflyder.home.feature.vpn.VpnDestination
import dev.proflyder.home.feature.vpn.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class, binding = binding<ToolbarContribution>())
@Inject
class VpnToolbarContribution internal constructor(
    private val hosts: VpnActivityHost.Factory,
    private val access: AppAccessSource,
) : ToolbarContribution {
    override val order: Int = 10

    override val module: AppModule = AppModule.VPN

    override fun attach(activity: AppCompatActivity, slots: ToolbarSlots, navigator: Navigator) {
        val toggle = slots.end.inflateChild(R.layout.vpn_toolbar_toggle) as MaterialButton
        val views = VpnToolbarViews(toggle, RevealBinder(slots.below.inflateChild(R.layout.vpn_route_banner)))
        val host = hosts.create(activity) { navigator.navigate(VpnDestination) }
        val model by activity.injectedViewModels<VpnToolbarViewModel>()
        host.bind(toggle)
        activity.lifecycleScope.launch {
            activity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                host.observePermissionRequests(this)
                launch { model.state.collect(views::render) }
                combine(navigator.chrome, access.state) { chrome, state ->
                    !chrome.overlay && state.allows(module)
                }.collect(views::show)
            }
        }
    }

    private fun ViewGroup.inflateChild(layout: Int): View =
        LayoutInflater.from(context).inflate(layout, this, false).also(::addView)
}

private class VpnToolbarViews(
    private val toggle: MaterialButton,
    private val banner: RevealBinder,
) {
    private var allowed = true

    fun render(state: VpnToolbarState) {
        VpnToolbarToggle.render(toggle, state.connection)
        banner.render(allowed && state.routeBannerVisible)
    }

    fun show(visible: Boolean) {
        allowed = visible
        toggle.isVisible = visible
        if (!visible) banner.render(false)
    }
}

package dev.proflyder.home.feature.vpn.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.connect.AutoConnectCoordinator
import dev.proflyder.home.feature.vpn.connect.HomeRoute
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine

internal data class VpnToolbarState(
    val connection: VpnConnection,
    val routeBannerVisible: Boolean,
)

@Inject
internal class VpnToolbarViewModel(
    controller: VpnController,
    coordinator: AutoConnectCoordinator,
) : StateViewModel<VpnToolbarState, Nothing, Nothing>(
    VpnToolbarState(controller.connection.value, coordinator.route.value.bannerVisible()),
) {
    init {
        launch {
            combine(controller.connection, coordinator.route, ::toolbarState).collect { state -> updateState { state } }
        }
    }

    override fun handle(intent: Nothing) = Unit
}

internal fun toolbarState(connection: VpnConnection, route: HomeRoute): VpnToolbarState =
    VpnToolbarState(connection, route.bannerVisible())

internal fun HomeRoute.bannerVisible(): Boolean = this == HomeRoute.CONNECTING

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(VpnToolbarViewModel::class)
@Inject
class VpnToolbarViewModelSource internal constructor(
    private val viewModel: () -> VpnToolbarViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

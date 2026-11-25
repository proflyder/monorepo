package dev.proflyder.home.core.presentation

import androidx.annotation.StringRes
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.navigation.open
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.NoticeAction
import dev.proflyder.home.core.ui.Notices

data class RouteNotice(
    @StringRes val message: Int,
    val route: Route? = null,
    @StringRes val action: Int = DesignR.string.ds_action_open,
)

fun Fragment.showNotice(notice: RouteNotice, message: CharSequence = getString(notice.message)) {
    val navigator = requireNavigator()
    val action = notice.route?.let { route -> NoticeAction(getString(notice.action)) { navigator.open(route) } }
    Notices.show(requireView(), message, long = action != null, action = action)
}

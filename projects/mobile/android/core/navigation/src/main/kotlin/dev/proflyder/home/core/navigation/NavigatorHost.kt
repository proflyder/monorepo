package dev.proflyder.home.core.navigation

import androidx.fragment.app.Fragment

/** Implemented by the single Activity so features can navigate without a DI lookup. */
interface NavigatorHost {
    val navigator: Navigator
}

fun Fragment.requireNavigator(): Navigator {
    val host = requireActivity()
    check(host is NavigatorHost) { "Activity must implement NavigatorHost" }
    return host.navigator
}

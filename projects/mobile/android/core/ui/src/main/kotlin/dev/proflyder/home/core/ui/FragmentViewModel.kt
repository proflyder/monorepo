package dev.proflyder.home.core.ui

import androidx.fragment.app.Fragment
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider

/**
 * Obtains a feature-owned ViewModel factory from the single Activity. The
 * Activity is the composition-root host; no fragment reaches into a global
 * singleton or Application-level graph.
 */
fun <VM : ViewModel> Fragment.viewModelFactory(viewModelClass: Class<VM>): ViewModelProvider.Factory {
    val host = requireActivity()
    check(host is ViewModelFactoryProvider) { "Activity must implement ViewModelFactoryProvider" }
    return host.viewModelFactory(viewModelClass)
}

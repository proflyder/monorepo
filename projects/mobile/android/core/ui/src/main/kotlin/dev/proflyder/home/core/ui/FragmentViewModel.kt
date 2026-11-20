package dev.proflyder.home.core.ui

import androidx.fragment.app.Fragment
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider

fun <VM : ViewModel> Fragment.viewModelFactory(viewModelClass: Class<VM>): ViewModelProvider.Factory {
    val host = requireActivity()
    check(host is ViewModelFactoryProvider) { "Activity must implement ViewModelFactoryProvider" }
    return host.viewModelFactory(viewModelClass)
}

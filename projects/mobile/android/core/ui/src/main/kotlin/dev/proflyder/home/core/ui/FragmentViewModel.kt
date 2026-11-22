package dev.proflyder.home.core.ui

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.activity.viewModels
import androidx.fragment.app.Fragment
import androidx.fragment.app.activityViewModels
import androidx.fragment.app.viewModels
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider

fun Context.injectedViewModelFactory(): ViewModelProvider.Factory {
    val host = applicationContext
    check(host is ViewModelFactoryProvider) { "Application must implement ViewModelFactoryProvider" }
    return host.viewModelFactory
}

inline fun <reified VM : ViewModel> Fragment.injectedViewModels(): Lazy<VM> =
    viewModels { requireContext().injectedViewModelFactory() }

inline fun <reified VM : ViewModel> Fragment.injectedActivityViewModels(): Lazy<VM> =
    activityViewModels { requireContext().injectedViewModelFactory() }

inline fun <reified VM : ViewModel> ComponentActivity.injectedViewModels(): Lazy<VM> =
    viewModels { injectedViewModelFactory() }

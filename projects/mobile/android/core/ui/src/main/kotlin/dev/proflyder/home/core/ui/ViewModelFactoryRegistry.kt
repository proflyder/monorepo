package dev.proflyder.home.core.ui

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.CreationExtras
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/** Implemented by MainActivity; backed by the Metro app graph. */
interface ViewModelFactoryProvider {
    fun viewModelFactory(viewModelClass: Class<out ViewModel>): ViewModelProvider.Factory
}

interface ViewModelFactoryContributor {
    val viewModelClass: Class<out ViewModel>
    fun createFactory(): ViewModelProvider.Factory
}

/**
 * The composition root collects feature-owned factories. Fragments ask the
 * Activity for the factory of their own ViewModel class; there is no global
 * service locator and no reflection-based graph.
 */
@SingleIn(AppScope::class)
@Inject
class ViewModelFactoryRegistry(
    contributors: Set<ViewModelFactoryContributor>,
) : ViewModelFactoryProvider {
    private val factories: Map<Class<out ViewModel>, ViewModelProvider.Factory> =
        contributors.associate { it.viewModelClass to it.createFactory() }

    override fun viewModelFactory(viewModelClass: Class<out ViewModel>): ViewModelProvider.Factory =
        factories[viewModelClass]
            ?: error("No ViewModel factory registered for ${viewModelClass.name}")
}

/** Simple factory for ViewModels with constructor-injected use cases. */
class SimpleViewModelFactory<VM : ViewModel>(
    private val create: () -> VM,
) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T = create() as T
}

/**
 * Factory for editor ViewModels that persist a non-secret draft in
 * SavedStateHandle across process recreation.
 */
class SavedStateViewModelFactory<VM : ViewModel>(
    private val create: (SavedStateHandle) -> VM,
) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T =
        create(extras.createSavedStateHandle()) as T

    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T =
        create(SavedStateHandle()) as T
}

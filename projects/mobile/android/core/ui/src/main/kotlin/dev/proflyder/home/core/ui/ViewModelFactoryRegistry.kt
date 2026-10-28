package dev.proflyder.home.core.ui

import android.view.View
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

/** Shared loading/empty/error/content rendering for list screens. */
class ListStateViews(
    private val progress: View,
    private val empty: View,
    private val error: View,
    private val content: View,
) {
    fun showLoading() {
        progress.visibility = View.VISIBLE
        empty.visibility = View.GONE
        error.visibility = View.GONE
        content.visibility = View.GONE
    }

    fun showEmpty(emptyText: CharSequence? = null) {
        progress.visibility = View.GONE
        empty.visibility = View.VISIBLE
        if (emptyText != null && empty is android.widget.TextView) {
            empty.text = emptyText
        }
        error.visibility = View.GONE
        content.visibility = View.GONE
    }

    fun showContent() {
        progress.visibility = View.GONE
        empty.visibility = View.GONE
        error.visibility = View.GONE
        content.visibility = View.VISIBLE
    }

    fun showError(message: CharSequence) {
        progress.visibility = View.GONE
        empty.visibility = View.GONE
        error.visibility = View.VISIBLE
        content.visibility = View.GONE
        if (error is android.widget.TextView) {
            error.text = message
        }
    }
}

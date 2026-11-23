package dev.proflyder.home.core.ui

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.CreationExtras
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Multibinds
import dev.zacsweers.metro.SingleIn
import kotlin.reflect.KClass

interface ViewModelFactoryProvider {
    val viewModelFactory: ViewModelProvider.Factory
}

interface ViewModelSource {
    fun create(): ViewModel
}

interface SavedStateViewModelFactory {
    fun create(savedState: SavedStateHandle): ViewModel
}

@ContributesTo(AppScope::class)
@BindingContainer
interface ViewModelMultibindings {
    @Multibinds(allowEmpty = true)
    fun viewModels(): Map<KClass<*>, ViewModel>

    @Multibinds(allowEmpty = true)
    fun viewModelSources(): Map<KClass<*>, ViewModelSource>

    @Multibinds(allowEmpty = true)
    fun savedStateViewModels(): Map<KClass<*>, SavedStateViewModelFactory>
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = dev.zacsweers.metro.binding<ViewModelProvider.Factory>())
@Inject
class ViewModelFactoryRegistry(
    private val viewModels: Map<KClass<*>, () -> ViewModel>,
    private val viewModelSources: Map<KClass<*>, ViewModelSource>,
    private val savedStateViewModels: Map<KClass<*>, SavedStateViewModelFactory>,
) : ViewModelProvider.Factory {
    override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T =
        build(modelClass) { extras.createSavedStateHandle() }

    override fun <T : ViewModel> create(modelClass: Class<T>): T =
        build(modelClass) {
            error("${modelClass.name} keeps saved state and needs CreationExtras from a SavedStateRegistryOwner")
        }

    private fun <T : ViewModel> build(modelClass: Class<T>, savedState: () -> SavedStateHandle): T {
        val key = modelClass.kotlin
        val model = viewModels[key]?.invoke()
            ?: viewModelSources[key]?.create()
            ?: savedStateViewModels[key]?.create(savedState())
            ?: error("No ViewModel binding for ${modelClass.name}")
        return requireNotNull(modelClass.cast(model))
    }
}

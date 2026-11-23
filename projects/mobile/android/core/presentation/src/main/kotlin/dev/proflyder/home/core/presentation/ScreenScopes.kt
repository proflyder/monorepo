package dev.proflyder.home.core.presentation

import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import dev.proflyder.home.core.ui.injectedViewModelFactory

class TabScopes : ViewModel() {
    private val stores = mutableMapOf<String, ViewModelStore>()

    fun owner(key: String): ViewModelStoreOwner = TabScope(stores.getOrPut(key, ::ViewModelStore))

    override fun onCleared() {
        stores.values.forEach(ViewModelStore::clear)
        stores.clear()
    }
}

private class TabScope(override val viewModelStore: ViewModelStore) : ViewModelStoreOwner

val Fragment.tabScopeKey: String
    get() = generateSequence(this, Fragment::getParentFragment)
        .toList()
        .asReversed()
        .joinToString(separator = "/") { "${it.tag}:${it.javaClass.name}" }

fun Fragment.tabScopeOwner(): ViewModelStoreOwner =
    ViewModelProvider(requireActivity())[TabScopes::class.java].owner(tabScopeKey)

inline fun <reified VM : ViewModel> Fragment.tabViewModels(): Lazy<VM> =
    viewModels(ownerProducer = { tabScopeOwner() }, factoryProducer = { requireContext().injectedViewModelFactory() })

inline fun <reified VM : ViewModel> Fragment.hostScreenViewModels(): Lazy<VM> =
    viewModels(ownerProducer = { requireParentFragment() }, factoryProducer = { requireContext().injectedViewModelFactory() })

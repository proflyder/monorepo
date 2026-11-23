package dev.proflyder.home.core.presentation

import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import dev.proflyder.home.core.ui.injectedViewModelFactory

abstract class FlowKey(val id: String)

interface FlowMember {
    val flow: FlowKey
}

class FlowScopes : ViewModel() {
    private val stores = mutableMapOf<String, ViewModelStore>()
    private val members = mutableMapOf<String, MutableSet<String>>()

    fun isActive(flow: FlowKey): Boolean = flow.id in stores

    fun join(flow: FlowKey, member: String): ViewModelStoreOwner {
        members.getOrPut(flow.id, ::mutableSetOf) += member
        return FlowScope(stores.getOrPut(flow.id, ::ViewModelStore))
    }

    fun leave(flow: FlowKey, member: String) {
        val remaining = members[flow.id] ?: return
        remaining -= member
        if (remaining.isNotEmpty()) return
        members -= flow.id
        stores.remove(flow.id)?.clear()
    }

    override fun onCleared() {
        stores.values.forEach(ViewModelStore::clear)
        stores.clear()
        members.clear()
    }
}

private class FlowScope(override val viewModelStore: ViewModelStore) : ViewModelStoreOwner

private class FlowMembership(
    private val fragment: Fragment,
    private val scopes: FlowScopes,
    private val flow: FlowKey,
    private val member: String,
) : DefaultLifecycleObserver {
    override fun onDestroy(owner: LifecycleOwner) {
        if (fragment.activity?.isChangingConfigurations == true) return
        scopes.leave(flow, member)
    }
}

val Fragment.flowKey: FlowKey
    get() = generateSequence(this, Fragment::getParentFragment).firstNotNullOfOrNull { (it as? FlowMember)?.flow }
        ?: error("${javaClass.simpleName} is not inside a FlowMember")

fun Fragment.flowScopeOwner(): ViewModelStoreOwner {
    val scopes = ViewModelProvider(requireActivity())[FlowScopes::class.java]
    val flow = flowKey
    val member = tabScopeKey
    lifecycle.addObserver(FlowMembership(this, scopes, flow, member))
    return scopes.join(flow, member)
}

inline fun <reified VM : ViewModel> Fragment.flowViewModel(): Lazy<VM> =
    viewModels(ownerProducer = { flowScopeOwner() }, factoryProducer = { requireContext().injectedViewModelFactory() })

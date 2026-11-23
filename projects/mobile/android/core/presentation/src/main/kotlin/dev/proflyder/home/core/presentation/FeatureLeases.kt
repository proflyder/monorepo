package dev.proflyder.home.core.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.Lease

fun <G : Any, VM : ViewModel> FeatureScope<G>.viewModel(build: (G) -> VM): VM {
    val lease = acquire()
    return build(lease.graph).also { it.addCloseable(lease) }
}

fun <VM : ViewModel> VM.holding(leases: List<Lease<*>>): VM = apply { leases.forEach(::addCloseable) }

class LeaseRelease(private val lease: Lease<*>) : DefaultLifecycleObserver {
    override fun onDestroy(owner: LifecycleOwner) = lease.close()
}

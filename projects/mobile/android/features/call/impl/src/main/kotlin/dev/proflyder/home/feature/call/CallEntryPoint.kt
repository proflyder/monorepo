package dev.proflyder.home.feature.call

import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.call.telecom.CallTelecomBridge
import dev.zacsweers.metro.Inject

internal object CallEntryPoint {
    @Volatile
    private var dependencies: Dependencies? = null

    fun install(dependencies: Dependencies) {
        this.dependencies = dependencies
    }

    fun require(): Dependencies = checkNotNull(dependencies) {
        "CallEntryPoint is not installed; the app graph must start CallEntryPointInstaller"
    }

    data class Dependencies(
        val controller: CallController,
        val telecomBridge: CallTelecomBridge,
    )
}

@Inject
internal class CallEntryPointInstaller(
    private val controller: CallController,
    private val telecomBridge: CallTelecomBridge,
) : AppStartable {
    override fun start() {
        CallEntryPoint.install(CallEntryPoint.Dependencies(controller, telecomBridge))
    }
}
